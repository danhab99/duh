use clap::error::Error;
use lib::{
    object_data::{Commit, Object},
    utils,
};
use std::{fs, path::PathBuf};

pub struct Repo {
    root_path: String,
}

impl Repo {
    pub fn at_root_path(root_path: Option<String>) -> Repo {
        let mut x: Commit;
        Repo {
            root_path: utils::find_repo_root(root_path).unwrap(),
        }
    }

    pub fn get_path_in_repo(&self, p: &str) -> PathBuf {
        let mut b = PathBuf::from(self.root_path.clone()).join(p);
        b.push(utils::REPO_METADATA_DIR_NAME);
        return b;
    }

    pub fn get_path_in_cwd(&self, p: &str) -> PathBuf {
        PathBuf::from(self.root_path.clone())
            .join(utils::get_cwd())
            .join(p)
    }

    pub fn initalize_at(root_path: Option<String>) -> Result<Repo, Error> {
        let repo = Repo::at_root_path(root_path);

        fs::create_dir_all(repo.get_path_in_repo("objects"))?;
        fs::create_dir_all(repo.get_path_in_repo("refs"))?;
        fs::write(repo.get_path_in_repo("config"), "# duh config")?;
        fs::write(repo.get_path_in_repo("HEAD"), "")?;

        Ok(repo)
    }

    pub fn track_new_file(&self, sub_paths: &Vec<String>) {
        for sub_path in sub_paths {
            let path = self.get_path_in_cwd(&sub_path);
        }
    }
}
