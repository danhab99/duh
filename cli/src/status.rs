use std::{collections::BTreeMap, error::Error};

use clap::clap_derive::Args;
use lib::{
    hash::Hash,
    objects::{Object, ObjectReference},
    space::Space,
};

#[derive(Args)]
#[command(about = "See what's changed, picked and new")]
pub struct StatusCommand {}

/// What a staged index entry will do to a file in the next commit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StagedChange {
    New,
    Modified,
    Deleted,
}

/// Files grouped by state, as repo-relative paths (sorted). A file can be
/// both staged and modified (edited again after staging).
#[derive(Default)]
pub struct StatusReport {
    /// In the index: will change in the next commit.
    pub staged: Vec<(String, StagedChange)>,
    /// Working copy differs from the staged version (or HEAD's, if unstaged).
    pub modified: Vec<String>,
    /// Tracked, not staged for deletion, but absent from the working tree.
    pub missing: Vec<String>,
    /// Tracked, not staged, and identical to HEAD.
    pub clean: Vec<String>,
    /// In the working tree but not tracked (hidden and ignored files excluded).
    pub untracked: Vec<String>,
}

impl StatusReport {
    /// Anything a commit, checkout or branch switch could lose.
    pub fn is_dirty(&self) -> bool {
        !(self.staged.is_empty() && self.modified.is_empty() && self.missing.is_empty())
    }

    /// Paths that will be in the next commit's tree (or HEAD's, unstaged).
    pub fn tracked(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .staged
            .iter()
            .filter(|(_, c)| *c != StagedChange::Deleted)
            .map(|(p, _)| p.clone())
            .chain(self.modified.iter().cloned())
            .chain(self.missing.iter().cloned())
            .chain(self.clean.iter().cloned())
            .collect();
        paths.sort();
        paths.dedup();
        paths
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
        return Err("this space has no working files (it's bare)".into());
    }

    // Expected content for each tracked file: the staged version if any,
    // otherwise HEAD's. `staged` records what the index entry does.
    let mut tracked: BTreeMap<String, (Hash, Option<StagedChange>)> = BTreeMap::new();
    let head_hash = space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;
    let head_files = if head_hash.is_zero() {
        Default::default()
    } else {
        space.get_commit_files(head_hash)?
    };
    for (path, version) in &head_files {
        tracked.insert(path.clone(), (*version, None));
    }
    for path in space.index_paths() {
        if let Some(version) = space.get_indexed_version(&path) {
            let change = if version.is_zero() {
                StagedChange::Deleted
            } else if head_files.contains_key(&path) {
                StagedChange::Modified
            } else {
                StagedChange::New
            };
            tracked.insert(path, (version, Some(change)));
        }
    }

    let mut report = StatusReport::default();
    for (path, (version, staged)) in &tracked {
        if let Some(change) = staged {
            report.staged.push((path.clone(), *change));
            if *change == StagedChange::Deleted {
                continue;
            }
        }

        let full_path = space.get_path_in_worktree(path)?;
        if !full_path.exists() {
            report.missing.push(path.clone());
            continue;
        }

        let working = Hash::digest_file_stream(&mut std::fs::File::open(&full_path)?)?;
        if content_hash(space, *version)? != Some(working) {
            report.modified.push(path.clone());
        } else if staged.is_none() {
            report.clean.push(path.clone());
        }
    }

    let tracked_now = report.tracked();
    report.untracked = crate::worktree::worktree_files(space)?
        .into_iter()
        .filter(|p| tracked_now.binary_search(p).is_err())
        .collect();
    Ok(report)
}

pub fn status(space: &mut Space, _cmd: &StatusCommand) -> Result<(), Box<dyn Error>> {
    let report = collect(space)?;
    let cwd = std::env::current_dir()?;

    let head_ref_name = match space.get_ref("HEAD".into())? {
        ObjectReference::Ref(name) => name,
        ObjectReference::Hash(hash) => format!("none (looking at version {})", &hash.to_string()[..12]),
        ObjectReference::AbbrevHash(s) => s,
    };
    println!("On draft {}", crate::colors::green(head_ref_name.as_str()));

    let staged: Vec<String> = report
        .staged
        .iter()
        .map(|(p, c)| {
            let p = space.display_path(&cwd, p);
            match c {
                StagedChange::New => format!("{} (new)", p),
                StagedChange::Modified => p,
                StagedChange::Deleted => format!("{} (deleted)", p),
            }
        })
        .collect();
    let rel = |paths: &Vec<String>| -> Vec<String> {
        paths.iter().map(|p| space.display_path(&cwd, p)).collect()
    };
    let sections: [(&str, Vec<String>, fn(&str) -> String); 5] = [
        ("Picked for the next save:", staged, crate::colors::green),
        ("Changed, not picked:", rel(&report.modified), crate::colors::yellow),
        ("Missing:", rel(&report.missing), crate::colors::red),
        ("New, not tracked yet:", rel(&report.untracked), crate::colors::red),
        ("Saved, unchanged:", rel(&report.clean), crate::colors::dim),
    ];
    let mut any = false;
    for (title, paths, color) in sections {
        if paths.is_empty() {
            continue;
        }
        any = true;
        println!();
        println!("{}", color(title));
        for p in paths {
            println!("  {}", color(&p));
        }
    }

    if !any {
        println!();
        println!(
            "{}",
            crate::colors::dim("Nothing here yet. Use `duh pick <file>` to start tracking files.")
        );
    }

    Ok(())
}
