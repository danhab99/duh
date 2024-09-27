use clap::Parser;
use lib::repo::Repo;
use sha2::{Sha256, Sha512, Digest};

use cli::{Cli, Commands};
use sha2::Sha512;

mod cli;
mod diff;
mod init;
mod status;
mod track;
mod commit;

fn main() {
    let cli = Cli::parse();

    let repo = Repo.at_root_path();

    match &cli.command {
        Commands::Init => init::init,
        Commands::Status  => status::status,
        Commands::Diff => diff::diff,
        Commands::Track(c)  => track::track(repo, c),
        Commands::Commit => commit::commit,
    }
}
