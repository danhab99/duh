use std::error::Error;

use clap::Parser;

use cli::{Cli, Commands};

mod branch;
mod check;
mod cli;
mod colors;
mod commit;
mod config;
mod copy;
mod diff;
mod init;
mod log;
mod mv;
mod reflog;
mod remote;
mod reset;
mod restore_cmd;
mod rm;
mod show;
mod stage;
mod status;
mod switch;
mod tag;
mod unstage;
mod when;

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
        Commands::Clone(c) => {
            let dest = match &c.directory {
                Some(d) => d.clone(),
                None => copy::default_clone_dir(&c.url)?,
            };
            let options = c
                .options
                .iter()
                .map(|o| lib::store::parse_option(o))
                .collect::<Result<Vec<_>, _>>()?;
            copy::clone(&c.url, &dest, c.branch.as_deref(), c.bare, &options)?;
            return Ok(());
        }
        _ => {
            let cwd = std::env::current_dir()?;
            let (metadata_dir, worktree) = lib::utils::find_duh_dir(cwd.to_str().unwrap())?;

            lib::space::Space::open(lib::store::local(&metadata_dir)?, worktree)?
        }
    };

    // Commands that change the space wait their turn, so duh Studio and
    // another duh never write to it at once. Looking never waits.
    let read_only = matches!(
        &cli.command,
        Commands::Status(_) | Commands::Diff(_) | Commands::Log(_) | Commands::Show(_) | Commands::Reflog(_) | Commands::Check(_)
    );
    let _lock = if read_only {
        None
    } else {
        Some(lib::ops::lock(&std::env::current_dir()?, std::time::Duration::from_secs(10))?)
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
        Commands::Log(c) => {
            log::log(&mut space, c)?;
        }
        Commands::Rm(c) => {
            rm::rm(&mut space, c)?;
        }
        Commands::Forget(c) => {
            rm::forget(&mut space, c)?;
        }
        Commands::Send(c) => {
            copy::send(&mut space, c)?;
        }
        Commands::Receive(c) => {
            copy::receive(&mut space, c)?;
        }
        Commands::Mv(c) => {
            mv::mv(&mut space, c)?;
        }
        Commands::Restore(c) => {
            restore_cmd::restore(&mut space, c)?;
        }
        Commands::Diff(c) => {
            diff::diff(&mut space, c)?;
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
        Commands::Clone(_) => unreachable!(),
        Commands::Remote(c) => {
            remote::remote(&mut space, c)?;
        }
        Commands::Tag(c) => {
            tag::tag(&mut space, c)?;
        }
        Commands::Reset(c) => {
            reset::reset(&mut space, c)?;
        }
        Commands::Check(c) => {
            check::check(&mut space, c)?;
        }
    };

    space.save_index()?;

    Ok(())
}

