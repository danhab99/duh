use std::error::Error;

use clap::Parser;
use lib::repo::Repo;

use cli::{Cli, Commands};

mod checkout;
mod cli;
mod commit;
mod init;
mod log;
mod show;
mod snapshot;
mod stage;
mod status;

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let mut repo = match &cli.command {
        Commands::Init => {
            init::init()?;
            return Ok(());
        }
        _ => Repo::at_root_path(None)?,
    };

    match &cli.command {
        Commands::Init => unreachable!(),
        Commands::Snapshot(c) => {
            snapshot::snapshot(&mut repo, c).expect("Unable to snapshot");
        }
        Commands::Stage(c) => {
            stage::stage(&mut repo, c).expect("Unable to stage");
        }
        Commands::Commit(c) => {
            commit::commit(&mut repo, c).expect("Unable to commit");
        }
        Commands::Show(c) => {
            show::show(&mut repo, c).expect("Unable to show commit");
        }
        Commands::Checkout(c) => {
            checkout::checkout(&mut repo, c).expect("Unable to checkout");
        }
        Commands::Log(c) => {
            log::log(&mut repo, c).expect("Unable to show log");
        }
        Commands::Status(c) => {
            status::status(&mut repo, c).expect("Unable to show status");
        }
    };

    repo.save_index()?;

    Ok(())
}
