use core::panic;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

use crate::error::NoRepo;

pub fn get_cwd() -> String {
    std::env::current_dir()
        .unwrap()
        .to_str()
        .unwrap_or_default()
        .to_owned()
}

pub const REPO_METADATA_DIR_NAME: &str = ".duh";
pub fn getRepoConfigFileName() -> String {
    return format!("{}/{}", REPO_METADATA_DIR_NAME, "config");
}
pub fn getRepoIgnoreFileName() -> String {
    return format!("{}/{}", REPO_METADATA_DIR_NAME, "ignore");
}

pub fn find_file(start_path: &str, target: &str) -> Result<String, Box<dyn Error>> {
    let mut path = PathBuf::from(start_path);

    loop {
        let mut p = path.clone();

        if p.eq(&PathBuf::from("/")) {
            return Err(Box::new(NoRepo { details: "mm".to_string() }));
        }

        p.push(target);
        println!("Checking path {}", p.display());

        if Path::new(p.to_str().unwrap_or("")).exists() {
            break;
        }

        if !path.pop() {
            break;
        }
    }

    Ok(format!("{}/{}", String::from(path.to_str().unwrap()), target))
}

pub fn hash_string(txt: String) -> Result<String, Box<dyn Error>> {
    let x = String::from_utf8(Sha256::digest(txt.clone()).to_vec())?;
    Ok(x)
}

pub fn hash_bytes(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    hex::encode(digest)
}
