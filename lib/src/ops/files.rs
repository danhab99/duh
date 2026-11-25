//! Bringing files back, renaming, deleting and forgetting them.

use std::fs;
use std::path::{Path, PathBuf};

use super::look::look;
use super::{worktree, OpResult};
use crate::file::FileOps;
use crate::objects::ObjectReference;
use crate::space::Space;

/// Bring back each file as it was: from your picks (when `from` is `None`
/// and it's picked), from the last save, or from version `from`. With
/// `as_path`, the one file is written there instead of over your copy.
/// Returns where each file was written.
pub fn bring_back(
    space: &mut Space,
    repo_paths: &[String],
    from: Option<&ObjectReference>,
    as_path: Option<&Path>,
) -> OpResult<Vec<(String, PathBuf)>> {
    if as_path.is_some() && repo_paths.len() != 1 {
        return Err("--as takes exactly one file".into());
    }
    let commit = match from {
        Some(r) => space.resolve_ref_name(r.clone())?,
        None => space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?,
    };
    let commit_files = if commit.is_zero() { Default::default() } else { space.get_commit_files(commit)? };

    let mut out = Vec::new();
    for p in repo_paths {
        let picked = from.is_none() && space.index.get(p).is_some_and(|h| !h.is_zero());
        let mut ops = FileOps::from_space(space);
        let mut reader = if picked {
            ops.open_staged(p.clone())?
        } else if commit_files.contains_key(p) {
            ops.open_file(p.clone(), commit)?
        } else if from.is_some() {
            return Err(format!("'{}' isn't in that version", p).into());
        } else {
            return Err(format!("'{}' isn't a file duh knows", p).into());
        };

        let dest = match as_path {
            Some(a) => a.to_path_buf(),
            None => space.get_path_in_worktree(p)?,
        };
        if let Some(parent) = dest.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        std::io::copy(&mut reader, &mut fs::File::create(&dest)?)?;
        out.push((p.clone(), dest));
    }
    Ok(out)
}

/// Rename or move a tracked file to `dest` (a repo path). The move costs
/// nothing in storage: the new path is diffed against the old one's saved
/// content.
pub fn rename(space: &mut Space, src: &str, dest: &str) -> OpResult<()> {
    let report = look(space)?;
    if report.tracked().binary_search(&src.to_string()).is_err() {
        return Err(format!("'{}' is not tracked", src).into());
    }
    let src_full = space.get_path_in_worktree(src)?;
    if !src_full.is_file() {
        return Err(format!("'{}' is missing from your files", src).into());
    }
    let dest_full = space.get_path_in_worktree(dest)?;
    if dest_full.exists() {
        return Err(format!("destination '{}' already exists", dest).into());
    }
    if let Some(parent) = dest_full.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&src_full, &dest_full)?;

    // A file that was only picked has no saved content to reuse.
    let head = space.get_head_commit_hash()?;
    let base = if !head.is_zero() && space.get_commit_files(head)?.contains_key(src) {
        Some(src.to_string())
    } else {
        None
    };
    let mut ops = FileOps::from_space(space);
    ops.stage_deletion(src.to_string())?;
    ops.stage_reader_from(dest.to_string(), base, Box::new(fs::File::open(&dest_full)?), None::<fn(_)>, None::<fn(_)>)?;
    space.save_index()?;
    Ok(())
}

/// Stop tracking files and delete them from the folder (earlier versions
/// stay in history). Refuses files with unsaved changes unless `force`.
/// Checks every file before touching any.
pub fn delete(space: &mut Space, repo_paths: &[String], force: bool) -> OpResult<()> {
    untrack(space, repo_paths, force, false)
}

/// Stop tracking files but keep them on disk.
pub fn forget(space: &mut Space, repo_paths: &[String]) -> OpResult<()> {
    untrack(space, repo_paths, true, true)
}

fn untrack(space: &mut Space, repo_paths: &[String], force: bool, keep_files: bool) -> OpResult<()> {
    let report = look(space)?;
    let tracked = report.tracked();
    for p in repo_paths {
        if tracked.binary_search(p).is_err() {
            return Err(format!("'{}' isn't a tracked file", p).into());
        }
        let has_changes = report.changed.contains(p) || report.picked.iter().any(|(s, _)| s == p);
        if has_changes && !force {
            return Err(format!(
                "'{}' has unsaved changes (--force deletes it anyway; `duh forget` keeps the file)",
                p
            )
            .into());
        }
    }
    for p in repo_paths {
        FileOps::from_space(space).stage_deletion(p.clone())?;
        if !keep_files {
            worktree::remove_file(space, p)?;
        }
    }
    space.save_index()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{pick::pick_file, save::save, testing};
    use super::*;

    #[test]
    fn rename_delete_and_bring_back() {
        let (dir, mut space) = testing::space();
        let a = testing::noise(1, 30_000);
        testing::write(&dir, "a.bin", &a);
        pick_file(&mut space, "a.bin", |_| {}).unwrap();
        let first = save(&mut space, "first", false).unwrap().version;

        rename(&mut space, "a.bin", "moved/a.bin").unwrap();
        assert!(dir.path().join("moved/a.bin").exists() && !dir.path().join("a.bin").exists());
        save(&mut space, "moved", false).unwrap();

        testing::write(&dir, "moved/a.bin", b"edited");
        assert!(delete(&mut space, &["moved/a.bin".into()], false).unwrap_err().to_string().contains("unsaved changes"));
        bring_back(&mut space, &["moved/a.bin".into()], None, None).unwrap();
        assert_eq!(fs::read(dir.path().join("moved/a.bin")).unwrap(), a);

        let copy = dir.path().join("old.bin");
        let from = ObjectReference::Hash(first);
        bring_back(&mut space, &["a.bin".into()], Some(&from), Some(&copy)).unwrap();
        assert_eq!(fs::read(&copy).unwrap(), a);
        assert!(bring_back(&mut space, &["nope".into()], None, None).unwrap_err().to_string().contains("isn't a file duh knows"));

        forget(&mut space, &["moved/a.bin".into()]).unwrap();
        assert!(dir.path().join("moved/a.bin").exists());
        delete(&mut space, &["moved/a.bin".into()], false).err().unwrap(); // no longer tracked
    }
}
