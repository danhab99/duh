use diff::Result as DiffResult;
use std::path::{Path, PathBuf};

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

pub fn get_cwd() -> String {
    std::env::current_dir()
        .unwrap()
        .to_str()
        .unwrap_or_default()
        .to_owned()
}

#[derive(PartialEq, Eq, Clone)]
pub enum DiffFragment {
    ADDED { offset: u64, body: Vec<u8> },
    UNCHANGED { offset: u64, len: u64 },
    DELETED { offset: u64, len: u64 },
}

pub fn diff_content(old: &[u8], new: &[u8]) -> Vec<DiffFragment> {
    let delta = diff::slice(old, new);

    // let mut frags = Vec::<DiffFragment>::new();
    let mut squished_frags = Vec::<DiffFragment>::new();

    for (i, d) in delta.iter().enumerate() {
        let l = squished_frags.last_mut();

        match (l, d) {
            (Some(DiffFragment::ADDED { offset: _, body }), DiffResult::Left(d)) => {
                println!("DIFF ADD: append {}", d);
                body.push(**d);
            }
            (Some(DiffFragment::UNCHANGED { offset, len }), DiffResult::Both(_, _)) => {
                *len += 1;
                println!("DIFF UNCHANGED: add {} {}", offset, len);
            }
            (Some(DiffFragment::DELETED { offset, len }), DiffResult::Right(_)) => {
                *len += 1;
                println!("DIFF DELETED: deleted {} {}", offset, len);
            }
            (None, DiffResult::Left(_)) => {
                println!("DIFF ADD: create {}", i);
                squished_frags.push(DiffFragment::DELETED {
                    offset: i as u64,
                    len: 1,
                })
            }
            (None, DiffResult::Both(_, _)) => {
                println!("DIFF UNCHANGED: create {}", i);
                squished_frags.push(DiffFragment::UNCHANGED {
                    offset: i as u64,
                    len: 1,
                })
            }
            (None, DiffResult::Right(b)) => {
                println!("DIFF UNCHANGED: create {}", i);
                squished_frags.push(DiffFragment::ADDED {
                    offset: i as u64,
                    body: vec![**b],
                })
            }
            _ => {}
        }
    }

    squished_frags
}
