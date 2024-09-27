use std::{error::Error, path::{Path, PathBuf}};
use sha2::{Digest, Sha256};

pub fn get_cwd() -> String {
    std::env::current_dir()
        .unwrap()
        .to_str()
        .unwrap_or_default()
        .to_owned()
}

pub const REPO_METADATA_DIR_NAME: &str = ".duh";
pub const REPO_CONFIG_FILE_NAME: &str = format!("{}{}", REPO_METADATA_DIR_NAME, "config").as_str();
pub const REPO_IGNORE_FILE_NAME: &str = format!("{}{}", REPO_METADATA_DIR_NAME, "ignore").as_str();

#[derive(Debug, Clone)]
pub struct NoRepo;

pub fn find_file(start_path: &str, target: &str) -> Result<String, Box<dyn Error>> {
    let mut path = PathBuf::from(start_path);

    loop {
        let mut p = path.clone();

        if p.eq(&PathBuf::from("/")) {
            return Err(NoRepo);
        }

        p.push(target.clone());
        println!("Checking path {}", p.display());

        if Path::new(p.to_str().unwrap_or("")).exists() {
            break;
        }

        path.pop();
    }

    Ok(String::from(path.to_str().unwrap()))
}

pub fn hash_string(txt: String) -> Result<String, Box<dyn Error>> {
    let x = String::from_utf8(Sha256::digest(txt.clone()).to_vec())?;
    Ok(x)
}
