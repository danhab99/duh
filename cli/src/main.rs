use clap::Parser;
use lib::repo::Repo;

use cli::{Cli, Commands};

mod cli;
mod diff;
mod init;
mod status;
mod track;
mod commit;

fn main() {
    let cli = Cli::parse();

    let repo = Repo::at_root_path(None).unwrap();

    match &cli.command {
        Commands::Init => init::init(),
        Commands::Status(c) => status::status(repo, c),
        Commands::Diff(c) => diff::diff(repo, c),
        Commands::Track(c)  => track::track(repo, c),
        Commands::Commit(c) => commit::commit(repo, c),
    }
}
