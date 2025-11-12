use clap::Parser;
use lib::repo::Repo;

use cli::{Cli, Commands};

mod cli;
mod diff;
mod init;
mod snapshot;

fn main() {
    let cli = Cli::parse();

    let repo = Repo::at_root_path(None).unwrap();

    match &cli.command {
        Commands::Init => init::init(),
        Commands::Diff(c) => diff::diff(repo, c),
        Commands::Snapshot(c) => snapshot::snapshot(repo, c),
    }
}
