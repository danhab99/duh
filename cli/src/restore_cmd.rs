use std::error::Error;
use std::fs;

use clap::clap_derive::Args;
use lib::file::FileOps;
use lib::objects::ObjectReference;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Restore working tree files, or unstage them with --staged")]
pub struct RestoreCommand {
    /// Files to restore, relative to the current directory
    #[arg(required = true)]
    pub paths: Vec<String>,

    /// Unstage: reset the index entry to HEAD
    #[arg(short = 'S', long = "staged")]
    pub staged: bool,

    /// Restore the working tree too (the default unless --staged)
    #[arg(short = 'W', long = "worktree")]
    pub worktree: bool,

    /// Commit to restore from (default: the index, then HEAD)
    #[arg(short = 's', long = "source")]
    pub source: Option<ObjectReference>,
}

pub fn restore(space: &mut Space, cmd: &RestoreCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let to_worktree = cmd.worktree || !cmd.staged;
    if cmd.staged && cmd.source.is_some() {
        return Err("--source with --staged isn't supported yet; unstage, then restore".into());
    }

    let mut repo_paths = Vec::new();
    for p in &cmd.paths {
        repo_paths.push(space.repo_path_from(&cwd, p)?);
    }

    if cmd.staged {
        for p in &repo_paths {
            if space.index.remove(p).is_none() {
                println!("{}", crate::colors::dim(&format!("{} was not staged", p)));
            } else {
                println!("Unstaged {}", p);
            }
        }
    }
    if !to_worktree {
        return Ok(());
    }

    let commit = match &cmd.source {
        Some(r) => space.resolve_ref_name(r.clone())?,
        None => space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?,
    };
    let commit_files = if commit.is_zero() {
        Default::default()
    } else {
        space.get_commit_files(commit)?
    };

    for p in &repo_paths {
        let staged = cmd.source.is_none() && space.index.get(p).is_some_and(|h| !h.is_zero());
        let mut fileops = FileOps::from_space(space);
        let mut reader = if staged {
            fileops.open_staged(p.clone())?
        } else if commit_files.contains_key(p) {
            fileops.open_file(p.clone(), commit)?
        } else {
            return Err(format!("pathspec '{}' did not match any file known to duh", p).into());
        };

        let full = space.get_path_in_worktree(p)?;
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        std::io::copy(&mut reader, &mut fs::File::create(&full)?)?;
        println!("Restored {}", p);
    }
    Ok(())
}
