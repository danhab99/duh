use std::error::Error;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use crate::hash::Hash;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Fragment(pub Vec<u8>);
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TreeRefStruct {
    pub name: String,
    pub hash: Hash,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileRefStruct {
    pub name: String,
    pub mode: u16,
    pub hash: Hash,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Person {
    pub name: String,
    pub email: String,
    pub timestamp: u64,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommitStruct {
    pub parent: Hash,
    pub tree: TreeRefStruct,
    pub message: String,
    pub comitter: Person,
    pub author: Person,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TreeStruct {
    pub trees: Vec<TreeRefStruct>,
    pub files: Vec<FileRefStruct>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileStruct {
    pub content_hash: Hash,
    pub fragments: Vec<Hash>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Object {
    Commit(CommitStruct),
    Tree(TreeStruct),
    File(FileStruct),
    Fragment(Fragment),
}

impl Object {
    pub fn to_msgpack(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.serialize(&mut Serializer::new(&mut buf)).unwrap();
        buf.to_vec()
    }

    pub fn from_msgpack(bin: Vec<u8>) -> Result<Object, Box<dyn Error>> {
        let mut d = Deserializer::new(bin.as_slice());
        let o = Object::deserialize(&mut d)?;
        Ok(o)
    }

    pub fn hash(&self) -> Result<(Vec<u8>, Hash), Box<dyn Error>> {
        let msgpack = self.to_msgpack();
        // let hash = Hash::from_string(String::from_utf8(Sha256::digest(msgpack.clone()).to_vec())?);
        let hash = Hash::from_string(hash_string(String::from_utf8(self.to_msgpack())?)?);

        return Ok((msgpack, hash));
    }
}

pub enum ObjectReference {
    Hash(Hash),
    Ref(String),
}

impl ObjectReference {
    pub fn ref_from_str(s: &str) -> Result<ObjectReference, Box<dyn Error>> {
        Ok(ObjectReference::Ref(String::from_str(s)?)) // TODO ref:
    }
}

