use std::error::Error;

use clap::clap_derive::Args;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Delete files and stop tracking them (earlier versions stay in history)")]
pub struct RmCommand {
    /// Tracked files to delete, relative to the current directory
    #[arg(required = true)]
    pub paths: Vec<String>,

    /// Keep the files on disk, just stop tracking them (= duh forget)
    #[arg(long = "cached", visible_alias = "keep")]
    pub cached: bool,

    /// Delete even if the files have unsaved changes
    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

#[derive(Args)]
#[command(about = "Stop tracking files but keep them on disk")]
pub struct ForgetCommand {
    /// Tracked files to forget, relative to the current directory
    #[arg(required = true)]
    pub paths: Vec<String>,
}

fn repo_paths(space: &Space, paths: &[String]) -> Result<Vec<String>, Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    paths.iter().map(|p| space.repo_path_from(&cwd, p)).collect()
}

pub fn forget(space: &mut Space, cmd: &ForgetCommand) -> Result<(), Box<dyn Error>> {
    let paths = repo_paths(space, &cmd.paths)?;
    lib::ops::files::forget(space, &paths)?;
    for p in paths {
        println!("Forgot {} (the file stays on disk)", p);
    }
    Ok(())
}

pub fn rm(space: &mut Space, cmd: &RmCommand) -> Result<(), Box<dyn Error>> {
    if cmd.cached {
        return forget(space, &ForgetCommand { paths: cmd.paths.clone() });
    }
    let paths = repo_paths(space, &cmd.paths)?;
    lib::ops::files::delete(space, &paths, cmd.force)?;
    for p in paths {
        println!("Deleted {}", p);
    }
    Ok(())
}
