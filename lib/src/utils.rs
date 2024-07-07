use std::path::{Path, PathBuf};

pub fn get_cwd() -> String {
    std::env::current_dir()
        .unwrap()
        .to_str()
        .unwrap_or_default()
        .to_owned()
}

pub const REPO_METADATA_DIR_NAME: &str = ".duh";

#[derive(Debug, Clone)]
pub struct NoRepo;

pub fn find_repo_root(start_path: Option<String>) -> Result<String, NoRepo> {
    let mut path = PathBuf::from(start_path.unwrap_or(get_cwd()));

    loop {
        let mut p = path.clone();

        if p.eq(&PathBuf::from("/")) {
            return Err(NoRepo);
        }

        p.push(REPO_METADATA_DIR_NAME);
        println!("Checking path {}", p.display());

        if Path::new(p.to_str().unwrap_or("")).exists() {
            break;
        }

        path.pop();
    }

    Ok(String::from(path.to_str().unwrap()))
}
