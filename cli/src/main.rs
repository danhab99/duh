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
            let mut repo = Repo::at_root_path(None)?;
            snapshot::snapshot(&mut repo, c).expect("Unable to snapshot");
        }
    };

    Ok(())
}
