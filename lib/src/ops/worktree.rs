//! The space's working files: which ones duh considers, writing a version
//! of a file out, and making the folder match a version.

use std::fs;
use std::path::{Path, PathBuf};

use globwalk::GlobWalkerBuilder;

use super::OpResult;
use crate::file::FileOps;
use crate::hash::Hash;
use crate::space::Space;

/// `.duhignore` patterns (relative to the worktree root) as globwalk
/// exclusions: each non-blank, non-comment line excludes matching paths;
/// a leading `!` re-includes them.
pub fn ignore_patterns(space: &Space) -> OpResult<Vec<String>> {
    let path = space.get_path_in_worktree(".duhignore")?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(fs::read_to_string(path)?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.strip_prefix('!') {
            Some(keep) => keep.to_string(),
            None => format!("!{}", l),
        })
        .collect())
}

/// True for paths duh never tracks implicitly: anything with a component
/// starting with '.' (including `.duh/` itself).
pub fn is_hidden(repo_path: &str) -> bool {
    repo_path.split('/').any(|c| c.starts_with('.'))
}

/// Repo paths of every file in the worktree that duh would consider,
/// skipping hidden entries and `.duhignore` matches. Sorted.
pub fn files(space: &Space) -> OpResult<Vec<String>> {
    let root = space.get_path_in_worktree("")?;
    let mut patterns = vec!["**/*".to_string()];
    patterns.extend(ignore_patterns(space)?);

    let mut files = Vec::new();
    for entry in GlobWalkerBuilder::from_patterns(&root, &patterns).build()? {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(&root) else {
            continue;
        };
        let repo_path = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if !is_hidden(&repo_path) {
            files.push(repo_path);
        }
    }
    files.sort();
    Ok(files)
}

/// Delete a worktree file and any directories it leaves empty (stopping at
/// the worktree root).
pub fn remove_file(space: &Space, repo_path: &str) -> OpResult<()> {
    let full = space.get_path_in_worktree(repo_path)?;
    if full.exists() {
        fs::remove_file(&full)?;
    }
    let root = space.get_path_in_worktree("")?;
    let mut dir = full.parent().map(PathBuf::from);
    while let Some(d) = dir {
        if d == root || fs::remove_dir(&d).is_err() {
            break;
        }
        dir = d.parent().map(PathBuf::from);
    }
    Ok(())
}

/// Write `repo_path` as of `commit` to `out`, or to its place in the
/// worktree when `out` is `None`. Streams; never holds the file in memory.
pub fn write_version(space: &mut Space, repo_path: &str, commit: Hash, out: Option<&Path>) -> OpResult<()> {
    let out_path = match out {
        Some(p) => p.to_path_buf(),
        None => space.get_path_in_worktree(repo_path)?,
    };
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut reader = FileOps::from_space(space).open_file(repo_path.to_string(), commit)?;
    std::io::copy(&mut reader, &mut fs::File::create(out_path)?)?;
    Ok(())
}

/// Make the worktree match commit `to`, coming from commit `from`: write
/// every file in `to` and delete files tracked by `from` that `to` lacks
/// (pruning directories left empty). Either hash may be zero (no commit).
pub fn match_version(space: &mut Space, from: Hash, to: Hash) -> OpResult<()> {
    let files_of = |space: &Space, h: Hash| -> OpResult<_> {
        Ok(if h.is_zero() { Default::default() } else { space.get_commit_files(h)? })
    };
    let from_files = files_of(space, from)?;
    let to_files = files_of(space, to)?;

    for path in from_files.keys().filter(|p| !to_files.contains_key(*p)) {
        remove_file(space, path)?;
    }

    let mut paths: Vec<&String> = to_files.keys().collect();
    paths.sort();
    for path in paths {
        write_version(space, path, to, None)?;
    }
    Ok(())
}
