use std::error::Error;
use std::fs;

use clap::clap_derive::Args;
use std::path::PathBuf;

use lib::file::FileOps;
use lib::hash::Hash;
use lib::objects::{Object, ObjectReference};
use lib::space::Space;

#[derive(Args)]
#[command(about = "Restore a file's contents from a specified commit into the working directory")]
pub struct CheckoutCommand {
    /// Path to the file to checkout
    #[arg(help = "Path to the file to restore (written to the working directory)")]
    pub file_path: String,

    /// Commit hash (64 chars) or the literal `HEAD`. Defaults to `HEAD`.
    #[arg(
        short = 'c',
        long = "commit",
        help = "Commit to read from (use `HEAD` or a full 64-character hash)"
    )]
    pub commit: Option<ObjectReference>,

    #[arg(long = "as", help = "checkout a file at a commit as a different name")]
    pub as_name: Option<String>,
}

pub fn checkout(
    space: &mut Space,
    cmd: &CheckoutCommand,
) -> Result<(), Box<dyn Error>> {
    // Resolve commit (default to HEAD)
    let target_hash = match &cmd.commit {
        Some(r) => space.resolve_ref_name(r.clone())?,
        None => space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?,
    };

    let cwd = std::env::current_dir()?;
    let repo_path = space.repo_path_from(&cwd, &cmd.file_path)?;

    // Ensure the commit exists and the file is present in that commit
    match space.get_object(target_hash)? {
        Some(Object::Commit(_c)) => {
            if !space.get_commit_files(target_hash)?.contains_key(&repo_path) {
                return Err(format!("file not found in commit: {}", cmd.file_path).into());
            }
        }
        _ => {
            return Err(format!("commit not found: {}", target_hash.to_string()).into());
        }
    }

    restore(space, &repo_path, target_hash, cmd.as_name.as_deref())?;

    println!(
        "{} {} @ {}",
        crate::colors::green("checked out"),
        crate::colors::cyan(&cmd.file_path),
        crate::colors::cyan(&target_hash.to_string())
    );
    Ok(())
}

/// Write `repo_path` as of `commit` to `out` (relative to the current
/// directory), or to its place in the worktree when `out` is `None`.
/// Streams the reconstruction without materializing it in memory.
pub fn restore(
    space: &mut Space,
    repo_path: &str,
    commit: Hash,
    out: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let out_path = match out {
        Some(p) => PathBuf::from(p),
        None => space.get_path_in_worktree(repo_path)?,
    };
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut fileops = FileOps::from_space(space);
    let mut reader = fileops.open_file(repo_path.to_string(), commit)?;
    let mut out_file = fs::File::create(out_path)?;
    std::io::copy(&mut reader, &mut out_file)?;
    Ok(())
}
