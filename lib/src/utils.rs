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

/// Returns true when `DUH_VERBOSE` env var is set to `1`, `true` or `yes`.
pub fn verbose_enabled() -> bool {
    std::env::var("DUH_VERBOSE")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes"
        })
        .unwrap_or(false)
}

/// Verbose logging macro. Prints only when `DUH_VERBOSE` is enabled.
/// Usage: `vlog!("details: {}", val);`
#[macro_export]
macro_rules! vlog {
    ($($arg:tt)*) => {
        if $crate::utils::verbose_enabled() {
            println!($($arg)*);
        }
    };
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
            return Err(Box::new(NoRepo {
                details: String::from(p.to_str().unwrap()),
            }));
        }

        p.push(target);
        vlog!("Checking path {}", p.display());

        if Path::new(p.to_str().unwrap_or("")).exists() {
            break;
        }

        if !path.pop() {
            break;
        }
    }

    Ok(format!(
        "{}/{}",
        String::from(path.to_str().unwrap()),
        target
    ))
}

pub fn hash_string(txt: String) -> Result<String, Box<dyn Error>> {
    let digest = Sha256::digest(txt.as_bytes());
    Ok(hex::encode(digest))
}

pub fn hash_bytes(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    hex::encode(digest)
}
