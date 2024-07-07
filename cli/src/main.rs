use std::path;

use clap::{Parser, Subcommand};
use lib::{diff, utils};

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
    Diff {
        old: String,
        new: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let cwd = utils::get_cwd();

    match &cli.command {
        Commands::Init => {
            let mut p = path::PathBuf::from(cwd);
            p.push(utils::REPO_METADATA_DIR_NAME);

            println!("Initialized new DUH directory {}", p.display());

            std::fs::create_dir(p.to_str().unwrap()).unwrap();
        }
        Commands::Status { wd } => {
            let path = wd.to_owned().unwrap_or(cwd.clone());

            if !path.starts_with(&cwd) {
                panic!("no repo found");
            }

            let root = utils::find_repo_root(Some(path.clone())).unwrap();

            let files = std::fs::read_dir(root)
                .unwrap()
                .map(|x| x.unwrap())
                .collect::<Vec<_>>();

            println!("STATUS {} {:?}", path, files);
        }
        Commands::Diff { old, new } => {
            let old_content = std::fs::read(old).unwrap();
            let new_content = std::fs::read(new).unwrap();

            // lib::diff_content(&old_content, &new_content);
            let diffs = diff::diff_content(&old_content, &new_content);

            for diff in diffs {
                match diff {
                    diff::DiffFragment::ADDED { offset, body } => {
                        println!("Added offset={} data={:02X?}", offset, body)
                    }
                    diff::DiffFragment::UNCHANGED { offset, len } => {
                        println!("Nothing changed from {} to {}", offset, len)
                    }
                    diff::DiffFragment::DELETED { offset, len } => {
                        println!("Deleted offset={} len={}", offset, len)
                    }
                }
            }
        }
    }
}
