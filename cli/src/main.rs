use std::error::Error;

use clap::Parser;
use lib::repo::Repo;

use cli::{Cli, Commands};

mod branch;
mod checkout;
mod cli;
mod colors;
mod commit;
mod config;
mod init;
mod log;
mod show;
mod stage;
mod status;
mod switch;
mod unstage;

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
        Commands::Stage(c) => {
            stage::stage(&mut repo, c).expect("Unable to stage");
        }
        Commands::Unstage(c) => {
            unstage::unstage(&mut repo, c).expect("Unable to unstage");
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
        Commands::Switch(c) => {
            switch::switch(&mut repo, c).expect("Unable to switch");
        }
        Commands::Branch(c) => {
            branch::branch(&mut repo, c).expect("Unable to branch");
        }
        Commands::Config(c) => {
            config::config(&repo, c).expect("Unable to run config");
        }
    };

    repo.save_index()?;

    Ok(())
}
