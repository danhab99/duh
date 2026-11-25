use crate::{hash::Hash, utils::hash_bytes};
use core::fmt;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, error::Error, str::FromStr};

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
pub struct TreeEntry {
    pub name: String,
    pub mode: u32,
    pub hash: Hash,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TreeStruct {
    pub entries: Vec<TreeEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommitStruct {
    pub parent: Hash,
    pub tree: Hash,
    pub message: String,
    pub comitter: Person,
    pub author: Person,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileStruct {
    pub content_hash: Hash,
    pub fragments: Vec<Hash>,
    /// Path whose version in the base commit these fragments diff against,
    /// when it isn't this file's own path (e.g. after a rename). Omitted
    /// from the encoding when `None`, so ordinary versions keep their
    /// original bytes and hashes, and older objects still decode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StagedFileStruct(Vec<u8>);

/// A file's whole content stored as plain fragments, so it can be read
/// without replaying the diffs that lead up to it. Solid copies sit beside
/// history rather than in it: they're found by content hash (see
/// `Space::solid_for`), and no commit, tree or file version refers to one.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SolidStruct {
    /// Hash of the bytes the fragments add up to.
    pub content_hash: Hash,
    /// Fragment objects and their lengths, in order.
    pub parts: Vec<(Hash, usize)>,
}

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

// #[derive(PartialEq, Eq, Clone, Debug, serde::Deserialize, serde::Serialize)]
// pub enum FileDiffFragment {
//     ADDED { body: Hash, len: usize },
//     UNCHANGED { len: usize },
//     DELETED { len: usize },
// }
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Object {
    Commit(CommitStruct),
    Tree(TreeStruct),
    File(FileStruct),
    Fragment(Fragment),
    StagedFileStruct(StagedFileStruct),
    FileDiffFragment(FileFragment),
    Solid(SolidStruct),
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
            Self::FileDiffFragment(_) => "filedifffragment",
            Self::Fragment(_) => "fragment",
            Self::Commit(_) => "commit",
            Self::StagedFileStruct(_) => "stagedfilestruct",
            Self::File(_) => "file",
            Self::Tree(_) => "tree",
            Self::Solid(_) => "solid",
        }
        .into()
    }
}

#[derive(PartialEq, Clone, Debug)]
pub enum ObjectReference {
    Hash(Hash),
    /// An abbreviated (prefix) hex hash, resolved lazily against the object store.
    AbbrevHash(String),
    Ref(String),
}

// impl ToString for ObjectReference {
//     fn to_string(&self) -> String {
//         match self {
//             ObjectReference::Hash(h) => h.to_string(),
//             ObjectReference::Ref(r) => format!("ref:{}", r),
//         }
//     }
// }

impl FromStr for ObjectReference {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let is_hex = !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit());
        if is_hex && s.len() == 64 {
            if let Ok(h) = Hash::from_string(s.to_string()) {
                return Ok(ObjectReference::Hash(h));
            }
        }
        if is_hex && s.len() >= 4 {
            return Ok(ObjectReference::AbbrevHash(s.to_lowercase()));
        }
        Ok(ObjectReference::Ref(s.to_string()))
    }
}

impl From<String> for ObjectReference {
    fn from(value: String) -> Self {
        return ObjectReference::from_str(&value.as_str()).unwrap();
    }
}

impl fmt::Display for ObjectReference {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hash(hash) => write!(out, "{}", hash.to_hex())?,
            Self::AbbrevHash(s) => write!(out, "{}", s)?,
            Self::Ref(name) => write!(out, "{}", name)?,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_struct_without_base_path_keeps_its_encoding() {
        #[derive(Serialize)]
        struct OldFileStruct {
            content_hash: Hash,
            fragments: Vec<Hash>,
        }
        let old = OldFileStruct { content_hash: Hash::new(), fragments: vec![Hash::new()] };
        let mut old_bytes = Vec::new();
        old.serialize(&mut Serializer::new(&mut old_bytes)).unwrap();

        let new = FileStruct { content_hash: Hash::new(), fragments: vec![Hash::new()], base_path: None };
        let mut new_bytes = Vec::new();
        new.serialize(&mut Serializer::new(&mut new_bytes)).unwrap();
        assert_eq!(old_bytes, new_bytes);

        let decoded = FileStruct::deserialize(&mut Deserializer::new(old_bytes.as_slice())).unwrap();
        assert_eq!(decoded.base_path, None);
    }

    #[test]
    fn file_struct_base_path_round_trips() {
        let f = FileStruct {
            content_hash: Hash::new(),
            fragments: vec![],
            base_path: Some("old/name.bin".into()),
        };
        let bytes = Object::File(f).to_msgpack();
        match Object::from_msgpack(bytes).unwrap() {
            Object::File(f) => assert_eq!(f.base_path.as_deref(), Some("old/name.bin")),
            _ => panic!("expected a file object"),
        }
    }
}
