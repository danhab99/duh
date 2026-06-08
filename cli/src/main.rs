use std::error::Error;

use clap::Parser;
use lib::space::Space;
use vfs::PhysicalFS;

use cli::{Cli, Commands};

mod branch;
mod checkout;
mod cli;
mod colors;
mod commit;
mod config;
mod init;
mod log;
mod reset;
mod show;
mod stage;
mod status;
mod switch;
mod unstage;
mod push;
mod pull;
mod tag;

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let mut space = match &cli.command {
        Commands::Init => {
            init::init()?;
            return Ok(());
        }
        _ => Space::at_root_path(None, PhysicalFS::new("/"))?,
    };

    match &cli.command {
        Commands::Init => unreachable!(),
        Commands::Stage(c) => {
            stage::stage(&mut space, c).expect("Unable to stage");
        }
        Commands::Unstage(c) => {
            unstage::unstage(&mut space, c).expect("Unable to unstage");
        }
        Commands::Commit(c) => {
            commit::commit(&mut space, c).expect("Unable to commit");
        }
        Commands::Show(c) => {
            show::show(&mut space, c).expect("Unable to show commit");
        }
        Commands::Checkout(c) => {
            checkout::checkout(&mut space, c).expect("Unable to checkout");
        }
        Commands::Log(c) => {
            log::log(&mut space, c).expect("Unable to show log");
        }
        Commands::Status(c) => {
            status::status(&mut space, c).expect("Unable to show status");
        }
        Commands::Switch(c) => {
            switch::switch(&mut space, c).expect("Unable to switch");
        }
        Commands::Branch(c) => {
            branch::branch(&mut space, c).expect("Unable to branch");
        }
        Commands::Config(c) => {
            config::config(&space, c).expect("Unable to run config");
        }
        Commands::Push(c) => {
            push::push(&mut space, c).expect("Unable to push");
        }
        Commands::Pull(c) => {
            pull::pull(&mut space, c).expect("Unable to pull");
        }
        Commands::Tag(c) => {
            tag::tag(&mut space, c).expect("Unable to tag");
        }
        Commands::Reset(c) => {
            reset::reset(&mut space, c).expect("Unable to reset");
        }
    };

    space.save_index()?;

    Ok(())
}
