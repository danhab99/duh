use std::error::Error;
use std::fs;

use std::path::PathBuf;

use lib::file::FileOps;
use lib::hash::Hash;
use lib::space::Space;

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

/// Make the worktree match commit `to`, coming from commit `from`: restore
/// every file in `to` and delete files tracked by `from` that `to` lacks
/// (pruning directories left empty). Either hash may be zero (no commit).
pub fn checkout_tree(space: &mut Space, from: Hash, to: Hash) -> Result<(), Box<dyn Error>> {
    let files_of = |space: &Space, h: Hash| -> Result<_, Box<dyn Error>> {
        Ok(if h.is_zero() {
            Default::default()
        } else {
            space.get_commit_files(h)?
        })
    };
    let from_files = files_of(space, from)?;
    let to_files = files_of(space, to)?;

    for path in from_files.keys().filter(|p| !to_files.contains_key(*p)) {
        crate::worktree::remove_file(space, path)?;
    }

    let mut paths: Vec<&String> = to_files.keys().collect();
    paths.sort();
    for path in paths {
        restore(space, path, to, None)?;
    }
    Ok(())
}
