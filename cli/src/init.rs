use lib::utils;
use std::{fs, path::PathBuf};

pub fn init() {
    let cwd = utils::get_cwd();
    let mut p = PathBuf::from(cwd);
    p.push(utils::REPO_METADATA_DIR_NAME);

    println!("Initialized new DUH directory {}", p.display());

    fs::create_dir(p.to_str().unwrap()).unwrap();
}
