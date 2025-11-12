use crate::{
    diff::{self, DiffFragment},
    hash::Hash,
    objects::{
        CommitStruct, FileRefStruct, FileStruct, Fragment, Object, ObjectReference, Person, TreeRefStruct, TreeStruct
    },
    utils::{self, REPO_METADATA_DIR_NAME, find_file, getRepoConfigFileName},
};
use std::{env, error::Error, fs::{self, File}, io::Read, path::PathBuf, str::FromStr, time::SystemTime};

use toml;

pub struct Repo {
    root_path: String,
    buffer_size: usize,
    me: Person,
}

pub type RepoError = Box<dyn Error>;
pub type RepoResult<T> = Result<T, RepoError>;

const BLOCK_SIZE = 512;

impl Repo {
    pub fn at_root_path(root_path: Option<String>) -> RepoResult<Repo> {
        let rp = match root_path {
            Some(x) => x,
            None => {
                let cwd = env::current_dir()?;
                let c = cwd.to_str().ok_or("cannot identify dir")?;
                String::from(c)
            }
        };

        let config_path = find_file(rp.as_str(), &getRepoConfigFileName())?;

        let content = fs::read(config_path)?;
        let decoded = String::from_utf8(content)?;
        let config = decoded.parse::<toml::Table>()?;

        let user_config = config
            .get("user")
            .ok_or("missing user config")?
            .as_table()
            .ok_or("user config isn't a table")?;

        let buffer_size = config
            .get("chunk_size")
            .ok_or("missing chunk_size")?
            .as_float()
            .ok_or("chunk_size is not a number")? as usize;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let rp = find_file(rp.as_str(), REPO_METADATA_DIR_NAME)?;

        Ok(Repo {
            root_path: rp,
            buffer_size,
            me: Person {
                name: String::from(
                    user_config
                        .get("name")
                        .ok_or("missing user.name")?
                        .as_str()
                        .unwrap_or("user.name is not a string"),
                ),
                email: String::from(
                    user_config
                        .get("email")
                        .ok_or("missing user.email")?
                        .as_str()
                        .unwrap_or("user.email is not a string"),
                ),
                timestamp: now,
            },
        })
    }

    fn get_path_in_repo(&self, p: &str) -> PathBuf {
        let mut b = PathBuf::from(self.root_path.clone()).join(p);
        b.push(utils::REPO_METADATA_DIR_NAME);
        fs::create_dir_all(p).unwrap();
        return b;
    }

    fn get_path_in_repo_str(&self, p: &str) -> String {
        let b = self.get_path_in_repo(p);
        let s = b.into_os_string().into_string().unwrap();
        return s;
    }

    fn get_path_in_cwd(&self, p: &str) -> PathBuf {
        PathBuf::from(self.root_path.clone())
            .join(utils::get_cwd())
            .join(p)
    }

    fn get_path_in_cwd_str(&self, p: &str) -> String {
        let b = self.get_path_in_cwd(p);
        let s = b.into_os_string().into_string().unwrap();
        return s;
    }

    pub fn initialize_at(root_path: String) -> RepoResult<Repo> {
        fs::create_dir_all(get_path_in_metadata("objects"))?;
        fs::create_dir_all(get_path_in_metadata("refs"))?;
        fs::write(get_path_in_metadata("config"), "# duh config")?;
        fs::write(get_path_in_metadata("HEAD"), "")?;

        Ok(Repo::at_root_path(Some(root_path))?)
    }

    fn get_object_path(&self, r: ObjectReference) -> RepoResult<PathBuf> {
        let hash = self.resolve_ref_name(r)?.to_string();
        let top = &hash[0..2];
        let bottom = &hash[2..hash.len()];

        Ok(self.get_path_in_repo(format!("objects/{}/{}", top, bottom).as_str()))
    }

    fn save_obj(&self, o: Object) -> RepoResult<Hash> {
        let (msgpack, hash) = o.hash()?;
        let path = self.get_object_path(ObjectReference::Hash(hash.clone()))?;
        fs::write(path, msgpack)?;
        Ok(hash)
    }

    fn read_object(&self, r: Hash) -> RepoResult<Option<Vec<u8>>> {
        let path = self.get_object_path(ObjectReference::Hash(r))?;
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read(path)?;
        return Ok(Some(content.to_vec()));
    }

    fn get_object(&self, r: Hash) -> RepoResult<Option<Object>> {
        let content = self.read_object(r)?;
        match content {
            Some(content) => Ok(Some(Object::from_msgpack(content)?)),
            None => Ok(None),
        }
    }

    fn get_ref_path(&self, name: &str) -> PathBuf {
        return self.get_path_in_repo(format!("refs/{}", name).as_str());
    }

    fn set_ref(&self, name: &str, r: ObjectReference) -> RepoResult<()> {
        let path = self.get_ref_path(name);
        fs::write(path, r.to_string())?;
        return Ok(());
    }

    pub fn get_ref(&self, name: String) -> RepoResult<ObjectReference> {
        let ref_path = self.get_path_in_repo(format!("refs/{}", name).as_str());
        let val = String::from_utf8(fs::read(ref_path)?)?;
        Ok(ObjectReference::from(val))
    }

    pub fn resolve_ref_name(&self, ref_name: ObjectReference) -> RepoResult<Hash> {
        match ref_name {
            ObjectReference::Hash(h) => Ok(h),
            ObjectReference::Ref(r) => {
                let n = self.get_ref(r)?;
                return Ok(self.resolve_ref_name(n)?);
            }
        }
    }

    pub fn get_commit_object(&self, r: ObjectReference) -> RepoResult<Option<CommitStruct>> {
        let hash = self.resolve_ref_name(r)?;
        let obj = self.get_object(hash)?.expect("commit not found");

        if let Object::Commit(commit) = obj {
            return Ok(Some(commit));
        } else {
            return Ok(None);
        }
    }

    pub fn build_file_struct(
        &self,
        content_hash: Hash,
        diff_fragments: &[DiffFragment],
    ) -> RepoResult<FileStruct> {
        let mut hashes = Vec::new();
        for frag in diff_fragments {
            let hash = self.save_obj(Object::Fragment(Fragment(*x)))?;
            hashes.push(hash);
        }

        let f = FileStruct {
            fragments: hashes,
            content_hash,
        };

        return Ok(f);
    }

    pub fn get_head_commit(&self) -> RepoResult<Option<CommitStruct>> {
        let r = ObjectReference::from_str("HEAD")?;
        let h = self.resolve_ref_name(r)?;
        self.get_commit_object(h)
    }

    fn get_head_version(&self, p: PathBuf) -> RepoResult<Option<FileStruct>> {
        let head = self.get_head_commit()?;

        if let Some(commit) = head {
            let tree: Option<&TreeRefStruct> = commit.trees.iter().find(|x| x.name == p[0]);

            while let Some(t) = tree {
                match self.get_object(t.hash)? {
                    None => break,
                    Some(next) => match next {
                        _ => {},
                        Object::Tree(t) => { tree = t.trees.iter().find(|x| x.name == p[1]) },
                        Object::File(file_struct) => {
                            return Ok(Some(file_struct));
                            
                        },
                    }
                }
            }
        }

        Ok(None)
    }

    pub fn build_tree_struct(&self, p: PathBuf) -> RepoResult<TreeStruct> {
        let mut root = p;
        // let mut tree: &TreeStruct;
        let mut tree_hash: Hash = Hash::new();

        loop {
            let name = String::from_str(root.file_name().unwrap().to_str().unwrap())?;

            if root.is_file() {
                let content_hash = {
                    let file = File::open(root)?;
                    Hash::digest_file_stream(&mut file)?
                };

                let file_struct = {
                    let file = File::open(root)?;

                    let head = self.get_head_commit()?;

                    let fragments = match head {
                        None => {
                            let mut body = Vec::new();
                            file.read(&mut body)?;
                            let frag = DiffFragment::ADDED { body };
                            vec![frag]
                        },
                        Some(head_commit_struct) => {
                            if let Some(last_version) = self.get_head_version(p)? {

                                let prev_fragments = last_version.fragments.into_iter().filter_map(|x| {
                                    let obj = self.get_object(x).unwrap();

                                    if let Some(Object::Fragment(frag)) = obj {
                                        Some(frag)
                                    } else {
                                        None
                                    }
                                }).collect::<Vec<_>>();

                                // Reconstruct previous version in memory
                                let mut prev_content = Vec::new();
                                let empty_old = std::io::Cursor::new(Vec::<u8>::new());
                                diff::apply_diff(empty_old, &prev_fragments, &mut prev_content)?;

                                // Now diff previous version against current file
                                let prev_cursor = std::io::Cursor::new(prev_content);
                                let current_file = File::open(&root)?;
                                let new_fragments = diff::diff_streams(prev_cursor, current_file, BLOCK_SIZE)?;

                            } else {}

                        },
                    };

                    self.build_file_struct(content_hash, fragments.as_slice())?
                };

                let file_hash = self.save_obj(Object::File(file_struct))?;

                let r = FileRefStruct {
                    name,
                    hash: file_hash,
                    mode: 0,
                };

                let t = TreeStruct {
                    trees: vec![],
                    files: vec![r],
                };

                tree_hash = self.save_obj(Object::Tree(t))?;
            } else if root.is_dir() {
                let tr = TreeRefStruct {
                    name,
                    hash: tree_hash,
                };

                let tree = TreeStruct { trees: vec![tr], files: vec![] };

                tree_hash = self.save_obj(Object::Tree(tree))?;
            }

            if !(root.pop()) {
                break;
            }
        }

        // let t = TreeStruct {
        //     trees: self.build_tree_struct()

        // };
    }

    pub fn commit_file(&self, p: PathBuf) -> RepoResult<TreeStruct> {
        assert!(!p.is_file());

        Ok(())
    }
}

fn get_path_in_metadata(path: &str) -> PathBuf {
    let mut p = PathBuf::new();
    p.push(REPO_METADATA_DIR_NAME);
    p.push(path);
    return p;
}
