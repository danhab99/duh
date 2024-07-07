use clap::Parser;

mod cli;
mod init;
mod status;
mod diff;

use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => init::init(),
        Commands::Status { wd } => status::status(wd.clone()),
        Commands::Diff { old, new } => diff::diff(old, new),
    }
}
