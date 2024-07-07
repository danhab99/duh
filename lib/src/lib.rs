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
        if squished_frags.len() <= 0 {
            match d {
                DiffResult::Left(_) => squished_frags.push(DiffFragment::DELETED {
                    offset: i as u64,
                    len: 1,
                }),
                DiffResult::Both(_, _) => squished_frags.push(DiffFragment::UNCHANGED {
                    offset: i as u64,
                    len: 1,
                }),
                DiffResult::Right(b) => squished_frags.push(DiffFragment::ADDED {
                    offset: i as u64,
                    body: vec![**b],
                }),
            }
            
            continue;
        }

        let l = squished_frags.last_mut().unwrap();

        match (l, d) {
            (
                DiffFragment::ADDED {
                    offset: _,
                    body: last_body,
                },
                DiffResult::Left(d),
            ) => {
                last_body.push(**d);
            }
            (
                DiffFragment::UNCHANGED {
                    offset: _,
                    len: last_len,
                },
                DiffResult::Both(_, _),
            ) => {
                *last_len += 1;
            }
            (
                DiffFragment::DELETED {
                    offset: _,
                    len: last_len,
                },
                DiffResult::Right(_),
            ) => {
                *last_len += 1;
            }
            _ => panic!("how??"),
        }
    }

    squished_frags
}
