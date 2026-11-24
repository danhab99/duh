use std::error::Error;
use std::fs;
use std::path::PathBuf;

use globwalk::GlobWalkerBuilder;
use lib::space::Space;

/// `.duhignore` patterns (relative to the worktree root) as globwalk
/// exclusions: each non-blank, non-comment line excludes matching paths;
/// a leading `!` re-includes them.
pub fn ignore_patterns(space: &Space) -> Result<Vec<String>, Box<dyn Error>> {
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
pub fn worktree_files(space: &Space) -> Result<Vec<String>, Box<dyn Error>> {
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
pub fn remove_file(space: &Space, repo_path: &str) -> Result<(), Box<dyn Error>> {
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
