use crate::{hash::Hash, utils::hash_bytes};
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error, str::FromStr};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Fragment(#[serde(with = "serde_bytes")] pub Vec<u8>);
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
    pub files: HashMap<String, Hash>,
    pub message: String,
    pub comitter: Person,
    pub author: Person,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileStruct {
    pub content_hash: Hash,
    pub fragments: Vec<Hash>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StagedFileStruct(Vec<u8>);

#[derive(PartialEq, Eq, Clone, Debug, serde::Deserialize, serde::Serialize)]
pub enum FileFragment {
    ADDED { body: Hash, len: usize },
    UNCHANGED { len: usize },
    DELETED { len: usize },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileVersion {
    pub content_hash: Hash,
    pub fragments: Vec<FileFragment>,
}

#[derive(PartialEq, Eq, Clone, Debug, serde::Deserialize, serde::Serialize)]
pub enum FileDiffFragment {
    ADDED { body: Hash, len: usize },
    UNCHANGED { len: usize },
    DELETED { len: usize },
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Object {
    Commit(CommitStruct),
    File(FileStruct),
    Fragment(Fragment),
    StagedFileStruct(StagedFileStruct),
    FileVersion(FileVersion),
    FileDiffFragment(FileDiffFragment),
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

    /// Deserialize from any `Read` source, avoiding a separate raw-bytes buffer.
    pub fn from_msgpack_reader<R: std::io::Read>(reader: R) -> Result<Object, Box<dyn Error>> {
        let mut d = Deserializer::new(reader);
        let o = Object::deserialize(&mut d)?;
        Ok(o)
    }

    pub fn hash(&self) -> Result<(Vec<u8>, Hash), Box<dyn Error>> {
        let msgpack = self.to_msgpack();
        let hash = Hash::from_string(hash_bytes(&msgpack))?;

        return Ok((msgpack, hash));
    }

    pub fn get_classification(self) -> String {
        match self {
            Self::FileVersion(_) => "fileversion",
            Self::FileDiffFragment(_) => "filedifffragment",
            Self::Fragment(_) => "fragment",
            Self::Commit(_) => "commit",
            Self::StagedFileStruct(_) => "stagedfilestruct",
            Self::File(_) => "file",
        }
        .into()
    }
}

pub enum ObjectReference {
    Hash(Hash),
    Ref(String),
}

impl ToString for ObjectReference {
    fn to_string(&self) -> String {
        match self {
            ObjectReference::Hash(h) => h.to_string(),
            ObjectReference::Ref(r) => format!("ref:{}", r),
        }
    }
}

impl FromStr for ObjectReference {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if let Some(target) = s.strip_prefix("ref:") {
            Ok(ObjectReference::Ref(target.trim().to_string()))
        } else {
            let h = Hash::from_str(s);
            Ok(ObjectReference::Hash(h))
        }
    }
}

impl From<String> for ObjectReference {
    fn from(value: String) -> Self {
        return ObjectReference::from_str(&value.as_str()).unwrap();
    }
}
