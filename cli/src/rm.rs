use std::error::Error;

use clap::clap_derive::Args;
use lib::file::FileOps;
use lib::space::Space;

use crate::status;

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

pub fn forget(space: &mut Space, cmd: &ForgetCommand) -> Result<(), Box<dyn Error>> {
    rm(space, &RmCommand { paths: cmd.paths.clone(), cached: true, force: false })
}

pub fn rm(space: &mut Space, cmd: &RmCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let report = status::collect(space)?;
    let tracked = report.tracked();

    // Validate everything before touching anything.
    let mut repo_paths = Vec::new();
    for p in &cmd.paths {
        let repo_path = space.repo_path_from(&cwd, p)?;
        if tracked.binary_search(&repo_path).is_err() {
            return Err(format!("'{}' isn't a tracked file", p).into());
        }
        let has_changes = report.modified.contains(&repo_path)
            || report.staged.iter().any(|(s, _)| *s == repo_path);
        if has_changes && !cmd.force && !cmd.cached {
            return Err(format!(
                "'{}' has unsaved changes (--force deletes it anyway; `duh forget` keeps the file)",
                p
            )
            .into());
        }
        repo_paths.push(repo_path);
    }

    for repo_path in repo_paths {
        FileOps::from_space(space).stage_deletion(repo_path.clone())?;
        if !cmd.cached {
            crate::worktree::remove_file(space, &repo_path)?;
        }
        if cmd.cached {
            println!("Forgot {} (the file stays on disk)", repo_path);
        } else {
            println!("Deleted {}", repo_path);
        }
    }
    Ok(())
}
