use std::{collections::BTreeMap, error::Error};

use clap::clap_derive::Args;
use lib::{
    hash::Hash,
    objects::{Object, ObjectReference},
    space::Space,
};

#[derive(Args)]
#[command(about = "Show status of tracked files (compare working copy -> index / HEAD)")]
pub struct StatusCommand {}

/// Tracked files grouped by state, as repo-relative paths (sorted).
/// A file can be both staged and modified (edited again after staging).
#[derive(Default)]
pub struct StatusReport {
    /// In the index: will change in the next commit.
    pub staged: Vec<String>,
    /// Working copy differs from the staged version (or HEAD's, if unstaged).
    pub modified: Vec<String>,
    /// Tracked but absent from the working tree.
    pub missing: Vec<String>,
    /// Tracked, not staged, and identical to HEAD.
    pub clean: Vec<String>,
}

impl StatusReport {
    /// Anything a commit, checkout or branch switch could lose.
    pub fn is_dirty(&self) -> bool {
        !(self.staged.is_empty() && self.modified.is_empty() && self.missing.is_empty())
    }
}

fn content_hash(space: &mut Space, version: Hash) -> Result<Option<Hash>, Box<dyn Error>> {
    Ok(match space.get_object(version)? {
        Some(Object::File(fv)) => Some(fv.content_hash),
        _ => None,
    })
}

pub fn collect(space: &mut Space) -> Result<StatusReport, Box<dyn Error>> {
    if space.worktree.is_none() {
        return Err("status needs a working tree (this space is bare)".into());
    }

    // Expected content for each tracked file: the staged version if any,
    // otherwise HEAD's.
    let mut tracked: BTreeMap<String, (Hash, bool)> = BTreeMap::new();
    let head_hash = space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;
    if !head_hash.is_zero() {
        for (path, version) in space.get_commit_files(head_hash)? {
            tracked.insert(path, (version, false));
        }
    }
    for path in space.index_paths() {
        if let Some(version) = space.get_indexed_version(&path) {
            tracked.insert(path, (version, true));
        }
    }

    let mut report = StatusReport::default();
    for (path, (version, staged)) in tracked {
        if staged {
            report.staged.push(path.clone());
        }

        let full_path = space.get_path_in_worktree(&path)?;
        if !full_path.exists() {
            report.missing.push(path);
            continue;
        }

        let working = Hash::digest_file_stream(&mut std::fs::File::open(&full_path)?)?;
        if content_hash(space, version)? != Some(working) {
            report.modified.push(path);
        } else if !staged {
            report.clean.push(path);
        }
    }
    Ok(report)
}

pub fn status(space: &mut Space, _cmd: &StatusCommand) -> Result<(), Box<dyn Error>> {
    let report = collect(space)?;
    let cwd = std::env::current_dir()?;

    let head_ref_name = match space.get_ref("HEAD".into())? {
        ObjectReference::Ref(name) => name,
        ObjectReference::Hash(hash) => format!("(detached at {})", hash.to_string()),
        ObjectReference::AbbrevHash(s) => s,
    };
    println!("On branch {}", crate::colors::green(head_ref_name.as_str()));

    let sections: [(&str, &Vec<String>, fn(&str) -> String); 4] = [
        ("Staged for commit:", &report.staged, crate::colors::green),
        ("Modified (not staged):", &report.modified, crate::colors::yellow),
        ("Deleted / missing:", &report.missing, crate::colors::red),
        ("Up to date:", &report.clean, crate::colors::dim),
    ];
    for (title, paths, color) in sections {
        if paths.is_empty() {
            continue;
        }
        println!();
        println!("{}", color(title));
        for p in paths {
            println!("  {}", color(&space.display_path(&cwd, p)));
        }
    }

    if report.staged.is_empty()
        && report.modified.is_empty()
        && report.missing.is_empty()
        && report.clean.is_empty()
    {
        println!();
        println!(
            "{}",
            crate::colors::dim(
                "No tracked files in index or HEAD. Use `duh stage <file>` to add files."
            )
        );
    }

    Ok(())
}
