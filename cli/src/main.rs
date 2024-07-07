use std::{borrow::Borrow, path};

use clap::{Parser, Subcommand};
use lib;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Find out what has changed in this repo
    Status {
        wd: Option<String>,
    },
    Init,
}

fn main() {
    let cli = Cli::parse();

    let cwd = lib::get_cwd();

    match &cli.command {
        Commands::Init => {
            let mut p = path::PathBuf::from(cwd);
            p.push(lib::REPO_METADATA_DIR_NAME);

            println!("Initialized new DUH directory {}", p.display());

            std::fs::create_dir(p.to_str().unwrap()).unwrap();
        },
        Commands::Status { wd } => {
            let path = wd.to_owned().unwrap_or(cwd.clone());

            if !path.starts_with(&cwd) {
                panic!("no repo found");
            }

            let root = lib::find_repo_root(Some(path.clone())).unwrap();

            let files = std::fs::read_dir(root)
                .unwrap()
                .map(|x| x.unwrap())
                .collect::<Vec<_>>();

            println!("STATUS {} {:?}", path, files);
        }
    }
}
