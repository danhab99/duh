use std::error::Error;

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
mod mv;
mod reflog;
mod remote;
mod reset;
mod rm;
mod show;
mod stage;
mod status;
mod switch;
mod tag;
mod unstage;
mod worktree;

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {}", colors::red("error:"), e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let mut space = match &cli.command {
        Commands::Init(c) => {
            init::init(c)?;
            return Ok(());
        }
        Commands::Copy(c) => {
            let cwd = std::env::current_dir()?;
            let result = lib::utils::find_duh_dir(cwd.to_str().unwrap());

            if result.is_err() {
                // Outside a duh space: treat as clone
                if let Some(ref url) = c.from {
                    let dest_dir = match c.as_branch.as_deref() {
                        Some(d) => d.to_string(),
                        None => copy::default_clone_dir(url)?,
                    };
                    copy::clone(url, &dest_dir, c.branch.as_deref(), c.bare)?;
                    return Ok(());
                } else {
                    return Err("must specify --from when cloning outside a duh space".into());
                }
            }

            // Inside a duh space: normal copy behavior
            let (metadata_dir, _worktree) = result?;
            let op =
                opendal::services::Fs::default().root(metadata_dir.to_str().unwrap());

            let fs = opendal::Operator::new(op)?.finish().blocking();

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

            let fs = opendal::Operator::new(op)?.finish().blocking();

            lib::space::Space::at_root_path(fs, worktree)?
        }
    };

    match &cli.command {
        Commands::Init(_) => unreachable!(),
        Commands::Stage(c) => {
            stage::stage(&mut space, c)?;
        }
        Commands::Unstage(c) => {
            unstage::unstage(&mut space, c)?;
        }
        Commands::Commit(c) => {
            commit::commit(&mut space, c)?;
        }
        Commands::Show(c) => {
            show::show(&mut space, c)?;
        }
        Commands::Checkout(c) => {
            checkout::checkout(&mut space, c)?;
        }
        Commands::Log(c) => {
            log::log(&mut space, c)?;
        }
        Commands::Rm(c) => {
            rm::rm(&mut space, c)?;
        }
        Commands::Mv(c) => {
            mv::mv(&mut space, c)?;
        }
        Commands::Reflog(c) => {
            reflog::reflog(&mut space, c)?;
        }
        Commands::Status(c) => {
            status::status(&mut space, c)?;
        }
        Commands::Switch(c) => {
            switch::switch(&mut space, c)?;
        }
        Commands::Branch(c) => {
            branch::branch(&mut space, c)?;
        }
        Commands::Config(c) => {
            config::config(&space, c)?;
        }
        Commands::Copy(_) => unreachable!(),
        Commands::Remote(c) => {
            remote::remote(&mut space, c)?;
        }
        Commands::Tag(c) => {
            tag::tag(&mut space, c)?;
        }
        Commands::Reset(c) => {
            reset::reset(&mut space, c)?;
        }
    };

    space.save_index()?;

    Ok(())
}

