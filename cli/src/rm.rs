use std::error::Error;

use clap::clap_derive::Args;
use lib::file::FileOps;
use lib::space::Space;

use crate::status;

#[derive(Args)]
#[command(about = "Remove files from the working tree and from the index")]
pub struct RmCommand {
    /// Tracked files to remove, relative to the current directory
    #[arg(required = true)]
    pub paths: Vec<String>,

    /// Stop tracking the files but keep them in the working tree
    #[arg(long = "cached")]
    pub cached: bool,

    /// Remove even if the files have uncommitted changes
    #[arg(short = 'f', long = "force")]
    pub force: bool,
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
            return Err(format!("pathspec '{}' did not match any tracked file", p).into());
        }
        let has_changes = report.modified.contains(&repo_path)
            || report.staged.iter().any(|(s, _)| *s == repo_path);
        if has_changes && !cmd.force && !cmd.cached {
            return Err(format!(
                "'{}' has uncommitted changes (use --force to remove anyway, or --cached to keep the file)",
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
        println!("rm '{}'", repo_path);
    }
    Ok(())
}
