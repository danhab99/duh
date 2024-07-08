use clap::Parser;
use sha2::{Sha256, Sha512, Digest};

mod repo;

mod cli;
use cli::{Cli, Commands};
use sha2::Sha512;

mod diff;
mod init;
mod status;
mod track;

fn main() {
    let cli = Cli::parse();

    let mut hasher = Sha512::new();
    hasher.update(b"hello world");
    let result = hasher.finalize().as_slice();

    match &cli.command {
        Commands::Init => init::init(),
        Commands::Status { wd } => status::status(wd.clone()),
        Commands::Diff { old, new } => diff::diff(old, new),
        Commands::Track { names } => track::track(names),
    }
}
