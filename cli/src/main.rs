use std::{
    error::Error,
    path::PathBuf,
};

use clap::Parser;

use cli::{Cli, Commands};

mod branch;
mod checkout;
mod cli;
mod colors;
mod commit;
mod config;
mod copy;
mod init;
mod log;
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
        Commands::Copy(c) => {
            let cwd = std::env::current_dir()?;
            let result = lib::utils::find_duh_dir(cwd.to_str().unwrap());

            if result.is_err() {
                // Outside a duh space: treat as clone
                if let Some(ref url) = c.from {
                    let dest_dir = c.as_branch.as_deref().unwrap_or_else(|| {
                        // Default to the last component of the URL
                        url.split('/')
                            .last()
                            .unwrap_or("repo")
                            .trim_end_matches(".duh")
                    });
                    copy::clone(url, dest_dir, c.branch.as_deref())?;
                    return Ok(());
                } else {
                    return Err("must specify --from when cloning outside a duh space".into());
                }
            }

            // Inside a duh space: normal copy behavior
            let (metadata_dir, _worktree) = result?;
            let op =
                opendal::services::Fs::default().root(metadata_dir.to_str().unwrap());

            let afs = opendal::Operator::new(op)?.finish();
            let fs = opendal::blocking::Operator::new(afs)?;

            let mut space = lib::space::Space::at_root_path(fs, _worktree)?;
            copy::copy(&mut space, c)?;
            space.save_index()?;
            return Ok(());
        }
        _ => {
            let cwd = std::env::current_dir()?;
            let (metadata_dir, worktree) = lib::utils::find_duh_dir(cwd.to_str().unwrap())?;

            let op =
                opendal::services::Fs::default().root(metadata_dir.to_str().unwrap());

            let afs = opendal::Operator::new(op)?.finish();
            let fs = opendal::blocking::Operator::new(afs)?;

            lib::space::Space::at_root_path(fs, worktree)?
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
        Commands::Copy(_) => unreachable!(),
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

