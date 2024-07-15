use crate::{
    diff::{self, DiffFragment},
    utils,
};
use std::{error::Error, fs, iter::zip, path::PathBuf};

use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub struct Repo {
    root_path: String,
}

impl Repo {
    pub fn at_root_path(root_path: Option<String>) -> Repo {
        Repo {
            root_path: utils::find_repo_root(root_path).unwrap(),
        }
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

    pub fn initalize_at(root_path: Option<String>) -> Result<Repo, Box<dyn Error>> {
        let repo = Repo::at_root_path(root_path);

        fs::create_dir_all(repo.get_path_in_repo("objects"))?;
        fs::create_dir_all(repo.get_path_in_repo("refs"))?;
        fs::write(repo.get_path_in_repo("config"), "# duh config")?;
        fs::write(repo.get_path_in_repo("HEAD"), "")?;

        Ok(repo)
    }

    fn get_object_path(&self, r: &str) -> Result<PathBuf, Box<dyn Error>> {
        let hash = self.resolve_ref_name(r)?;
        let top = hex::encode(&hash[0..2]);
        let bottom = hex::encode(&hash[2..hash.len()]);

        Ok(self.get_path_in_repo(format!("objects/{}/{}", top, bottom).as_str()))
    }

    fn save_obj(&self, o: Object) -> Result<(), Box<dyn Error>> {
        let msgpack = o.to_msgpack();
        let hash = String::from_utf8(Sha256::digest(msgpack.clone()).to_vec())?;
        let path = self.get_object_path(hash.as_str())?;
        fs::write(path, msgpack)?;
        Ok(())
    }

    fn read_object(&self, r: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        let path = self.get_object_path(r)?;
        let content = fs::read(path)?;
        return Ok(content.to_vec());
    }

    fn get_object(&self, r: &str) -> Result<Object, Box<dyn Error>> {
        let content = self.read_object(r)?;
        let o = Object::from_msgpack(content)?;
        return Ok(o);
    }

    fn get_ref_path(&self, name: &str) -> PathBuf {
        return self.get_path_in_repo(format!("refs/{}", name).as_str());
    }

    fn set_ref(&self, name: &str, hash: Vec<u8>) -> Result<(), Box<dyn Error>> {
        let path = self.get_ref_path(name);
        fs::write(path, hash)?;
        return Ok(());
    }

    fn get_ref(&self, name: &str) -> Result<&str, Box<dyn Error>> {
        let ref_path = self.get_path_in_repo(format!("refs/{}", name).as_str());
        let ref_hash = String::from_utf8(fs::read(ref_path)?)?;
        return Ok(ref_hash.as_str());
    }

    fn resolve_ref_name(&self, refname: &str) -> Result<&str, Box<dyn Error>> {
        if refname.starts_with("ref:") {
            let parts = refname.split(":").collect::<Vec<_>>();
            let r = self.get_ref(parts[1])?;
            return self.resolve_ref_name(r);
        } else {
            self.get_ref(refname)
        }
    }

    fn get_commit_at_head(&self) -> Result<CommitStruct, Box<dyn Error>> {
        let head_hash = self.resolve_ref_name("HEAD")?;
        let head_commit: Object = self.get_object(head_hash)?;
        match head_commit {
            Object::Commit(c) => Ok(c),
            _ => Err("object is not a commit".into()),
        }
    }

    fn diff_objs(&self, old_r: &str, new_r: &str) -> Result<Vec<DiffFragment>, Box<dyn Error>> {
        let old = self.read_object(old_r)?;
        let new = self.read_object(new_r)?;
        Ok(diff::diff_content(old.as_slice(), new.as_slice()))
    }

    fn track_file(&self, relative_path: &str) -> Result<(), Box<dyn Error>> {
        let absolute_path = self.get_path_in_cwd(relative_path);
        let relative_to_repo_path = absolute_path.strip_prefix(self.root_path)?;

        let head_commit = self.get_commit_at_head()?;

        for p in relative_to_repo_path.iter() {
            let path_name = p.to_str().unwrap_or("");
            let tree = head_commit
                .trees
                .iter()
                .filter(|tree| path_name == tree.name)
                .collect::<Vec<_>>()
                .first();

            match tree {
                Some(tree) => {

                },
                None => {},
            }
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Person {
    name: String,
    email: String,
    timestamp: u64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct CommitStruct {
    parent: String,
    trees: Vec<TreeStruct>,
    message: String,
    comitter: Person,
    author: Person,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct TreeStruct {
    name: String,
    trees: Vec<TreeStruct>,
    files: Vec<FileStruct>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct FileStruct {
    name: String,
    mode: u16,
    fragments: Vec<FragmentStruct>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct FragmentStruct {
    offset: usize,
    content: Vec<u8>,
}
#[derive(Debug, Serialize, Deserialize)]
pub enum Object {
    Commit(CommitStruct),
    Tree(TreeStruct),
    File(FileStruct),
    Fragment(FragmentStruct),
}

impl Object {
    fn get_type(&self) -> u8 {
        match self {
            Self::Commit(_) => 0u8,
            Self::Tree(_) => 1u8,
            Self::File(_) => 2u8,
            Self::Fragment(_) => 3u8,
        }
    }

    fn to_msgpack(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.serialize(&mut Serializer::new(&mut buf)).unwrap();
        buf.to_vec()
    }

    fn from_msgpack(bin: Vec<u8>) -> Result<Object, Box<dyn Error>> {
        let mut d = Deserializer::new(bin.as_slice());
        let o = Object::deserialize(&mut d)?;
        Ok(o)
    }
}
