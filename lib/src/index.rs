use crate::hash::Hash;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::{error::Error, fs, path::PathBuf, str::FromStr};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StagedFile {
    pub content_hash: Hash,
    pub file_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Index {
    pub staged_files: Vec<StagedFile>,
}

pub struct IndexManager {
    index_path: String,
    index: Option<Index>,
}

impl IndexManager {
    pub fn new(p: &str) -> IndexManager {
        IndexManager {
            index_path: String::from(p),
            index: None,
        }
    }

    pub fn transaction<F, R>(&self, handler: F) -> Result<R, Box<dyn Error>> where F: Fn(&mut Index) -> Result<R, Box<dyn Error>>{
        let bin = fs::read(PathBuf::from_str(self.index_path.as_str())?)?;
        let mut d = Deserializer::new(bin.as_slice());
        let mut index = Index::deserialize(&mut d)?;

        let res = handler(&mut index);

        let mut buf = Vec::new();
        index.serialize(&mut Serializer::new(&mut buf))?;
        
        return res;
    }
}
