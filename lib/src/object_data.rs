use rmp::encode as msgp_w;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    path::{self, Path},
};

use serde::{Deserialize, Serialize};

use crate::diff::DiffFragment;

#[repr(u8)]
pub enum ObjectType {
    Commit = 0,
    Tree,
    File,
    Fragment,
}

type BoxDynError = Box<dyn Error>;

pub trait Object {
    fn get_type(&self) -> ObjectType;
    fn marshal(&self) -> Result<Vec<u8>, BoxDynError>;
    // fn unmarshal(&self, data: &[u8]) -> Result<u64, BoxDynError>;
}

fn generate_hash<T: Object>(o: &T) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update([o.get_type() as u8]);
    hasher.update(o.marshal().as_mut().unwrap());

    hasher.finalize().to_vec()
}

fn generate_hashes<T: Object>(u: &Vec<T>) -> Vec<Vec<u8>> {
    let mut o = u.iter().map(|f| generate_hash(f)).collect::<Vec<_>>();
    o.sort();
    return o;
}

pub trait ObjectReference<T: Object> {
    fn from_hash(hash: &[u8]) -> T;
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct Person {
    name: String,
    email: String,
    timestamp: u64,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct Commit {
    parent: String,
    trees: Vec<Tree>,
    message: String,
    comitter: Person,
    author: Person,
}

impl Object for Commit {
    fn get_type(&self) -> ObjectType {
        ObjectType::Commit
    }

    fn marshal(&self) -> Result<Vec<u8>, BoxDynError> {
        let mut buf = Vec::<u8>::new();

        msgp_w::write_str(&mut buf, "parent")?;
        msgp_w::write_str(&mut buf, &self.parent)?;

        msgp_w::write_str(&mut buf, "message")?;
        msgp_w::write_str(&mut buf, &self.message)?;

        msgp_w::write_str(&mut buf, "comitter")?;
        msgp_w::write_map_len(&mut buf, 2)?;
        msgp_w::write_str(&mut buf, "name")?;
        msgp_w::write_str(&mut buf, &self.comitter.name)?;
        msgp_w::write_str(&mut buf, "email")?;
        msgp_w::write_str(&mut buf, &self.comitter.email)?;
        msgp_w::write_str(&mut buf, "timestamp")?;
        msgp_w::write_u64(&mut buf, self.comitter.timestamp)?;

        msgp_w::write_str(&mut buf, "author")?;
        msgp_w::write_map_len(&mut buf, 2)?;
        msgp_w::write_str(&mut buf, "name")?;
        msgp_w::write_str(&mut buf, &self.author.name)?;
        msgp_w::write_str(&mut buf, "email")?;
        msgp_w::write_str(&mut buf, &self.author.email)?;
        msgp_w::write_str(&mut buf, "timestamp")?;
        msgp_w::write_u64(&mut buf, self.author.timestamp)?;

        let hashes = generate_hashes(&self.trees);
        msgp_w::write_array_len(&mut buf, hashes.len() as u32)?;
        for hash in hashes {
            msgp_w::write_array_len(&mut buf, hash.len() as u32)?;
            for e in hash {
                msgp_w::write_u8(&mut buf, e)?;
            }
        }

        Ok(buf)
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct Tree {
    name: String,
    trees: Vec<Tree>,
    files: Vec<File>,
}

impl Object for Tree {
    fn get_type(&self) -> ObjectType {
        ObjectType::Tree
    }

    fn marshal(&self) -> Result<Vec<u8>, BoxDynError> {
        let mut buf = Vec::<u8>::new();

        msgp_w::write_str(&mut buf, "name")?;
        msgp_w::write_str(&mut buf, &self.name)?;

        msgp_w::write_str(&mut buf, "trees")?;
        for hash in generate_hashes(&self.trees) {
            msgp_w::write_array_len(&mut buf, (*hash).len() as u32)?;
            for e in hash {
                msgp_w::write_u8(&mut buf, e)?;
            }
        }

        msgp_w::write_str(&mut buf, "files")?;
        for hash in generate_hashes(&self.files) {
            msgp_w::write_array_len(&mut buf, (*hash).len() as u32)?;
            for e in hash {
                msgp_w::write_u8(&mut buf, e)?;
            }
        }

        Ok(buf)
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct File {
    name: String,
    mode: u16,
    fragments: Vec<Fragment>,
}

fn from_file_path(path: String, prev_file: Option<File>) -> Result<File, Box<dyn Error>> {
    let content = fs::read(path)?;

    let fragments = Vec::new();

    match prev_file {
        None => {
            fragments.push(DiffFragment::ADDED { offset: 0, body: content })
        }
        Some(f) => {
            
        }
    }

    Ok(File {
        name: Path::new(path.as_str()).file_name().unwrap().to_str().unwrap().to_string(),
        mode: 0u16,
        fragments:
    })
}

impl Object for File {
    fn get_type(&self) -> ObjectType {
        ObjectType::File
    }

    fn marshal(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut buf = Vec::<u8>::new();

        msgp_w::write_str(&mut buf, "name")?;
        msgp_w::write_str(&mut buf, &self.name)?;

        msgp_w::write_str(&mut buf, "mode")?;
        msgp_w::write_u16(&mut buf, self.mode)?;

        let hashes = generate_hashes(&self.fragments);

        msgp_w::write_str(&mut buf, "frags")?;
        msgp_w::write_array_len(&mut buf, hashes.len() as u32)?;
        for h in hashes {
            msgp_w::write_array_len(&mut buf, h.len() as u32)?;
            for b in h.iter() {
                msgp_w::write_u8(&mut buf, *b)?;
            }
        }

        Ok(buf)
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct Fragment {
    offset: u64,
    content: Vec<u8>,
}

impl Object for Fragment {
    fn get_type(&self) -> ObjectType {
        ObjectType::Fragment
    }

    fn marshal(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(self.content.to_vec())
    }
}
