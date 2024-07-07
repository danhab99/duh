use clap::Parser;

mod repo;

mod cli;
use cli::{Cli, Commands};

mod init;
mod status;
mod diff;

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => init::init(),
        Commands::Status { wd } => status::status(wd.clone()),
        Commands::Diff { old, new } => diff::diff(old, new),
    }
}
