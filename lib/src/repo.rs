use crate::{
    diff::{self, DiffFragment},
    utils,
    hash::Hash
};
use std::{error::Error, fs, path::PathBuf};

use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub struct Repo {
    root_path: String,
}

pub enum ObjectReference {
    Hash(Hash),
    Ref(String),
}

macro_rules! get_objects {
    ($self:ident, $collection:expr, $variant:path) => {
        $collection
            .iter()
            .map(|hash| {
                let obj = $self.get_object(*hash).unwrap();
                match obj {
                    $variant(obj) => Some(obj),
                    _ => None,
                }
            })
            .flatten()
            .collect::<Vec<_>>()
    };
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

    fn get_object_path(&self, r: ObjectReference) -> Result<PathBuf, Box<dyn Error>> {
        let hash = self.resolve_ref_name(r)?.to_string();
        let top = hex::encode(&hash[0..2]);
        let bottom = hex::encode(&hash[2..hash.len()]);

        Ok(self.get_path_in_repo(format!("objects/{}/{}", top, bottom).as_str()))
    }

    fn save_obj(&self, o: Object) -> Result<(), Box<dyn Error>> {
        let msgpack = o.to_msgpack();
        let hash = Hash::from_string(String::from_utf8(Sha256::digest(msgpack.clone()).to_vec())?);
        let path = self.get_object_path(ObjectReference::Hash(hash))?;
        fs::write(path, msgpack)?;
        Ok(())
    }

    fn read_object(&self, r: Hash) -> Result<Vec<u8>, Box<dyn Error>> {
        let path = self.get_object_path(ObjectReference::Hash(r))?;
        let content = fs::read(path)?;
        return Ok(content.to_vec());
    }

    fn get_object(&self, r: Hash) -> Result<Object, Box<dyn Error>> {
        let content = self.read_object(r)?;
        let o = Object::from_msgpack(content)?;
        return Ok(o);
    }

    fn get_ref_path(&self, name: &str) -> PathBuf {
        return self.get_path_in_repo(format!("refs/{}", name).as_str());
    }

    fn set_ref(&self, name: &str, r: ObjectReference) -> Result<(), Box<dyn Error>> {
        let path = self.get_ref_path(name);
        fs::write(
            path,
            match r {
                ObjectReference::Hash(h) => h.to_string(),
                ObjectReference::Ref(r) => format!("ref:{}", r),
            },
        )?;
        return Ok(());
    }

    fn get_ref(&self, name: String) -> Result<ObjectReference, Box<dyn Error>> {
        let ref_path = self.get_path_in_repo(format!("refs/{}", name).as_str());
        let val = String::from_utf8(fs::read(ref_path)?)?;
        if val.starts_with("ref:") {
            Ok(ObjectReference::Ref(val))
        } else {
            let v = val.as_bytes();
            let h = Hash(v.try_into()?);
            Ok(ObjectReference::Hash(h))
        }
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

    fn commit_get_trees(&self, c: CommitStruct) -> Vec<TreeStruct> {
        get_objects!(self, c.trees, Object::Tree)
    }

    fn tree_get_trees(&self, t: TreeStruct) -> Vec<TreeStruct> {
        get_objects!(self, t.trees, Object::Tree)
    }

    fn tree_get_files(&self, t: TreeStruct) -> Vec<FileStruct> {
        get_objects!(self, t.files, Object::File)
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
    trees: Vec<Hash>,
    message: String,
    comitter: Person,
    author: Person,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct TreeStruct {
    name: String,
    trees: Vec<Hash>,
    files: Vec<Hash>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct FileStruct {
    name: String,
    mode: u16,
    fragments: Vec<Hash>,
}
#[derive(Debug, Serialize, Deserialize)]
pub enum Object {
    Commit(CommitStruct),
    Tree(TreeStruct),
    File(FileStruct),
}

impl Object {
    fn get_type(&self) -> u8 {
        match self {
            Self::Commit(_) => 0u8,
            Self::Tree(_) => 1u8,
            Self::File(_) => 2u8,
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
