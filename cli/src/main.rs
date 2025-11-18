use std::error::Error;

use clap::Parser;
use lib::repo::Repo;

use cli::{Cli, Commands};

mod cli;
mod init;
mod snapshot;

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();


    match &cli.command {
        Commands::Init => {
            let _ = init::init()?;
        }
        Commands::Snapshot(c) => {
            let repo = Repo::at_root_path(None)?;
            snapshot::snapshot(repo, c).expect("Unable to snapshot");
        }
    };

    Ok(())
}
