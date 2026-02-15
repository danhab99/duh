use crate::{
    diff::DiffFragment,
    hash::Hash,
    objects::{
        CommitStruct, FileDiffFragment, FileVersion, Fragment, Object, ObjectReference, Person,
    },
    utils::{self, find_file, getRepoConfigFileName, REPO_METADATA_DIR_NAME},
};
use std::{collections::HashMap, error::Error, fs, io::Read, path::PathBuf, time::SystemTime};

use toml;

pub struct Repo {
    root_path: String,
    buffer_size: usize,
    me: Person,
    index: HashMap<String, Hash>,
}

pub type RepoError = Box<dyn Error>;
pub type RepoResult<T> = Result<T, RepoError>;

pub const BLOCK_SIZE: usize = 512;

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
            .as_integer()
            .ok_or("chunk_size is not a number")? as usize;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let rp = find_file(rp.as_str(), REPO_METADATA_DIR_NAME)?;

        let mut r = Repo {
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
            index: HashMap::new(),
        };

        let index_file_path = r.get_path_in_repo("index");

        let mut index_file = fs::File::open(index_file_path).unwrap();
        let mut contents = String::new();
        index_file.read_to_string(&mut contents)?;
        for line in contents.lines() {
            let parts = line.split("=").collect::<Vec<_>>();
            assert!(parts.len() == 2);

            let filepath_part = parts[0];
            let hash_part = parts[1];

            r.index.insert(
                filepath_part.to_string(),
                Hash::from_string(hash_part.to_string()),
            );
        }

        Ok(r)
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

    pub fn get_path_in_cwd(&self, p: &str) -> PathBuf {
        PathBuf::from(utils::get_cwd()).join(p)
        // PathBuf::from(self.root_path.clone())
        //     .join(utils::get_cwd())
        //     .join(p)
    }

    pub fn get_path_in_cwd_str(&self, p: &str) -> String {
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

    pub fn save_obj(&self, o: Object) -> RepoResult<Hash> {
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

    pub fn get_object(&self, r: Hash) -> RepoResult<Option<Object>> {
        let content = self.read_object(r)?;
        match content {
            Some(content) => Ok(Some(Object::from_msgpack(content)?)),
            None => Ok(None),
        }
    }

    fn get_ref_path(&self, name: &str) -> PathBuf {
        return self.get_path_in_repo(format!("refs/{}", name).as_str());
    }

    pub fn set_ref(&self, name: &str, r: ObjectReference) -> RepoResult<()> {
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

    pub fn stage_file<I: Iterator<Item = DiffFragment>>(
        &mut self,
        filepath: String,
        fragments: I,
        content_hash: Hash,
    ) -> RepoResult<Hash> {
        let file_diff_fragments = fragments.into_iter().map(|fragment| {
            let fdf = match fragment {
                DiffFragment::ADDED { body } => {
                    let frag_hash = self.save_obj(Object::Fragment(Fragment(body))).unwrap();
                    FileDiffFragment::ADDED { body: frag_hash }
                }
                DiffFragment::UNCHANGED { len } => FileDiffFragment::UNCHANGED { len },
                DiffFragment::DELETED { len } => FileDiffFragment::DELETED { len },
            };

            let hash = self.save_obj(Object::FileDiffFragment(fdf)).unwrap();

            return hash;
        });

        let version = FileVersion {
            content_hash: content_hash,
            fragments: file_diff_fragments.collect::<Vec<_>>(),
        };

        let version_hash = self.save_obj(Object::FileVersion(version))?;

        self.index.insert(filepath, version_hash);

        Ok(version_hash)
    }

    pub fn commit(&mut self, message: String) -> RepoResult<Hash> {
        let head_commit = self.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;

        let commit = CommitStruct {
            parent: head_commit,
            message: message,
            comitter: self.me.clone(),
            author: self.me.clone(),
            files: self.index.clone(),
        };

        let commit_hash = self.save_obj(Object::Commit(commit))?;

        self.set_ref("HEAD", ObjectReference::Hash(commit_hash))?;

        Ok(commit_hash)
    }
}

fn get_path_in_metadata(path: &str) -> PathBuf {
    let mut p = PathBuf::new();
    p.push(REPO_METADATA_DIR_NAME);
    p.push(path);
    return p;
}
