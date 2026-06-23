use std::{
    error::Error,
    path::{Path, PathBuf},
};

use clap::Parser;
use lib::space::Space;
use vfs::PhysicalFS;

use cli::{Cli, Commands};
use opendal::{self, Builder};

mod branch;
mod checkout;
mod cli;
mod colors;
mod commit;
mod config;
mod init;
mod log;
mod copy;
mod reflog;
mod remote;
mod reset;
mod show;
mod stage;
mod status;
mod switch;
mod tag;
mod unstage;

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let mut space = match &cli.command {
        Commands::Init => {
            init::init()?;
            return Ok(());
        }
        _ => {
            // Space::at_root_path()?
            let root_dir = find_duh_dir(std::env::current_dir()?.as_path());

            let op = opendal::services::Fs::default()
                .root(root_dir.and_then(|x| x.as_os_str().to_str()).unwrap());
            let fs = opendal::blocking::Operator::from(fs.build()?);

            lib::space::Space::at_root_path(fs)?
        }
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
        Commands::Copy(c) => {
            copy::copy(&mut space, c).expect("Unable to copy");
        }
        Commands::Remote(c) => {
            remote::remote(&mut space, c).expect("Unable to manage remote");
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

pub fn find_duh_dir(current_dir: &Path) -> Option<&Path> {
    let mut duh_dir = PathBuf::from(current_dir);
    duh_dir.push(".duh");

    if Path::from(duh_dir).is_dir() {
        return Some(current_dir.as_path());
    } else {
        current_dir.parent().and_then(find_duh_dir)
    }
}
