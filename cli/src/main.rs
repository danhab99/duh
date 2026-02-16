use std::error::Error;

use clap::Parser;
use lib::repo::Repo;

use cli::{Cli, Commands};

mod cli;
mod commit;
mod init;
mod snapshot;
mod stage;
mod show;
mod log;
mod checkout;

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let mut repo = Repo::at_root_path(None)?;

    match &cli.command {
        Commands::Init => {
            let _ = init::init()?;
        }
        Commands::Snapshot(c) => snapshot::snapshot(&mut repo, c).expect("Unable to snapshot"),
        Commands::Stage(c) => stage::stage(&mut repo, c).expect("Unable to stage"),
        Commands::Commit(c) => commit::commit(&mut repo, c).expect("Unable to commit"),
        Commands::Show(c) => show::show(&mut repo, c).expect("Unable to show commit"),
        Commands::Checkout(c) => checkout::checkout(&mut repo, c).expect("Unable to checkout"),
        Commands::Log(c) => log::log(&mut repo, c).expect("Unable to show log"),
    };

    repo.save_index()?;

    Ok(())
}
