use crate::{
    diff::diff_content,
    hash::Hash,
    index::{IndexManager, StagedFile},
    objects::{
        CommitStruct, FileRefStruct, FileStruct, Fragment, Object, ObjectReference, Person,
        TreeRefStruct, TreeStruct,
    },
    utils::{self, find_file, getRepoConfigFileName, REPO_METADATA_DIR_NAME},
};
use std::{env, error::Error, fs, io::Write, path::PathBuf, str::FromStr, time::SystemTime};

use sha2::{Digest, Sha256};
use toml;

pub struct Repo {
    root_path: String,
    buffer_size: usize,
    me: Person,
    index: IndexManager,
}

impl Repo {
    pub fn at_root_path(root_path: Option<String>) -> Result<Repo, Box<dyn Error>> {
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
            index: IndexManager::new(rp.as_str()),
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

    pub fn initalize_at(root_path: String) -> Result<Repo, Box<dyn Error>> {
        fs::create_dir_all(get_path_in_metadata("objects"))?;
        fs::create_dir_all(get_path_in_metadata("refs"))?;
        fs::write(get_path_in_metadata("config"), "# duh config")?;
        fs::write(get_path_in_metadata("HEAD"), "")?;

        Ok(Repo::at_root_path(Some(root_path))?)
    }

    fn get_object_path(&self, r: ObjectReference) -> Result<PathBuf, Box<dyn Error>> {
        let hash = self.resolve_ref_name(r)?.to_string();
        let top = &hash[0..2];
        let bottom = &hash[2..hash.len()];

        Ok(self.get_path_in_repo(format!("objects/{}/{}", top, bottom).as_str()))
    }

    fn save_obj(&self, o: Object) -> Result<Hash, Box<dyn Error>> {
        let (msgpack, hash) = o.hash()?;
        let path = self.get_object_path(ObjectReference::Hash(hash.clone()))?;
        fs::write(path, msgpack)?;
        Ok(hash)
    }

    fn read_object(&self, r: Hash) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        let path = self.get_object_path(ObjectReference::Hash(r))?;
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read(path)?;
        return Ok(Some(content.to_vec()));
    }

    fn get_object(&self, r: Hash) -> Result<Option<Object>, Box<dyn Error>> {
        let content = self.read_object(r)?;
        match content {
            Some(content) => Ok(Some(Object::from_msgpack(content)?)),
            None => Ok(None),
        }
    }

    fn get_ref_path(&self, name: &str) -> PathBuf {
        return self.get_path_in_repo(format!("refs/{}", name).as_str());
    }

    fn set_ref(&self, name: &str, r: ObjectReference) -> Result<(), Box<dyn Error>> {
        let path = self.get_ref_path(name);
        fs::write(path, r.to_string())?;
        return Ok(());
    }

    fn get_ref(&self, name: String) -> Result<ObjectReference, Box<dyn Error>> {
        let ref_path = self.get_path_in_repo(format!("refs/{}", name).as_str());
        let val = String::from_utf8(fs::read(ref_path)?)?;
        Ok(ObjectReference::from(val))
    }

    fn resolve_ref_name(&self, refname: ObjectReference) -> Result<Hash, Box<dyn Error>> {
        match refname {
            ObjectReference::Hash(h) => Ok(h),
            ObjectReference::Ref(r) => {
                let n = self.get_ref(r)?;
                return Ok(self.resolve_ref_name(n)?);
            }
        }
    }

    pub fn stage_file(&mut self, fp: &str) -> Result<Hash, Box<dyn Error>> {
        let file_path = PathBuf::from_str(fp)?;
        let content = fs::read(fp)?;

        let mut digester = Sha256::new();
        digester.write(&content)?;
        let d = digester.finalize();
        let content_hash = Hash::from_slice(d.as_slice());

        fs::copy(
            fp,
            format!(
                "{}/staged/{}",
                self.get_path_in_repo("index").to_str().unwrap(),
                content_hash.to_string()
            ),
        )?;

        self.index.transaction(move |index| {
            index.staged_files.push(StagedFile {
                content_hash,
                file_path: file_path.clone(),
            });

            Ok(())
        })?;

        Ok(content_hash)
    }

    fn build_file_diff(
        &self,
        new_file: ObjectReference,
        old_file: Option<ObjectReference>,
    ) -> Result<Hash, Box<dyn Error>> {
        let confirmed_old_file = if let Some(x) = old_file {
            x
        } else {
            return self.resolve_ref_name(new_file);
        };

        let new_file_content = self.read_object(self.resolve_ref_name(new_file)?)?.unwrap();
        let old_file_content = self
            .read_object(self.resolve_ref_name(confirmed_old_file)?)?
            .unwrap();

        let diff_parts = diff_content(old_file_content.as_slice(), new_file_content.as_slice());

        let f = FileStruct {
            fragments: diff_parts.iter().try_fold(Vec::new(), |mut acc, part| {
                let p = Object::Fragment(Fragment(part.to_owned()));
                let h = self.save_obj(p)?;
                acc.push(h);
                Ok::<Vec<Hash>, Box<dyn Error>>(acc)
            })?,
            content_hash: Hash::digest_slice(new_file_content.as_slice())?,
        };

        Ok(self.save_obj(Object::File(f))?)
    }

    fn build_tree_diff(
        &self,
        current_path: &str,
        old_tree: Option<TreeStruct>,
    ) -> Result<Hash, Box<dyn Error>> {
        let current_path_parts = current_path.split("/").collect::<Vec<_>>();
        let current_path_parts_len = current_path_parts.len();

        let current_path_parts_for_filter = current_path_parts.clone();
        let allowed_files = self.index.transaction(move |index| {
            Ok(index
                .staged_files
                .iter()
                .filter(|staged_file| {
                    let staged_path_parts = staged_file
                        .file_path
                        .iter()
                        .take(current_path_parts_len)
                        .map(|x| x.to_str().unwrap())
                        .collect::<Vec<_>>();

                    return current_path_parts_for_filter == staged_path_parts;
                })
                .cloned()
                .collect::<Vec<StagedFile>>())
        })?;

        let mut t = TreeStruct {
            trees: Vec::new(),
            files: Vec::new(),
        };

        for x in allowed_files {
            let search_path = PathBuf::from_iter(x.file_path.iter().take(current_path_parts.len()));
            let name = String::from(search_path.iter().last().unwrap().to_str().unwrap());

            if search_path.is_file() {
                t.files.push(FileRefStruct {
                    name,
                    mode: 0u16,
                    hash: self.build_file_diff(ObjectReference::Hash(x.content_hash), None)?,
                });
            } else if search_path.is_dir() {
                let nt = self.build_tree_diff(
                    name.as_str(),
                    old_tree
                        .as_ref()
                        .and_then(|old_tree| old_tree.trees.iter().find(|x| x.name == name))
                        .and_then(|next_old_tree| {
                            self.get_object(next_old_tree.hash).ok().flatten()
                        })
                        .map(|next_old_tree| match next_old_tree {
                            Object::Tree(tree) => tree,
                            _ => panic!("not a tree"),
                        }),
                )?;
                t.trees.push(TreeRefStruct { name, hash: nt });
            } else {
                panic!("what is this");
            }
        }

        Ok(self.save_obj(Object::Tree(t))?)
    }
}

fn get_path_in_metadata(path: &str) -> PathBuf {
    let mut p = PathBuf::new();
    p.push(REPO_METADATA_DIR_NAME);
    p.push(path);
    return p;
}
