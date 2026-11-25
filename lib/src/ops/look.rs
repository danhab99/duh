//! What's changed, picked, missing and new: `duh look`.

use std::collections::BTreeMap;

use super::{worktree, OpResult};
use crate::hash::Hash;
use crate::objects::{Object, ObjectReference};
use crate::space::Space;

/// What a picked entry will do to a file in the next save.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickedChange {
    New,
    Modified,
    Deleted,
}

/// Files grouped by state, as repo paths (sorted). A file can be both picked
/// and changed (edited again after picking).
#[derive(Default, Debug)]
pub struct Look {
    /// The open draft, or `None` when looking at a version directly.
    pub draft: Option<String>,
    /// The last save on it (zero before the first save).
    pub head: Hash,
    /// Picked: will change in the next save.
    pub picked: Vec<(String, PickedChange)>,
    /// Working copy differs from the picked version (or the last save's).
    pub changed: Vec<String>,
    /// Tracked, not picked for deletion, but gone from the folder.
    pub missing: Vec<String>,
    /// Tracked, not picked, and identical to the last save.
    pub unchanged: Vec<String>,
    /// In the folder but not tracked (hidden and ignored files excluded).
    pub new: Vec<String>,
}

impl Look {
    /// Anything a save, a switch of drafts or receiving could lose.
    pub fn is_dirty(&self) -> bool {
        !(self.picked.is_empty() && self.changed.is_empty() && self.missing.is_empty())
    }

    /// Paths that will be in the next save (or the last one, if nothing is
    /// picked). Sorted.
    pub fn tracked(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .picked
            .iter()
            .filter(|(_, c)| *c != PickedChange::Deleted)
            .map(|(p, _)| p.clone())
            .chain(self.changed.iter().cloned())
            .chain(self.missing.iter().cloned())
            .chain(self.unchanged.iter().cloned())
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }
}

fn content_hash(space: &Space, version: Hash) -> OpResult<Option<Hash>> {
    Ok(match space.get_object(version)? {
        Some(Object::File(fv)) => Some(fv.content_hash),
        _ => None,
    })
}

pub fn look(space: &mut Space) -> OpResult<Look> {
    if space.worktree.is_none() {
        return Err("this space has no working files (it's bare)".into());
    }

    let mut report = Look {
        draft: match space.get_ref("HEAD".into())? {
            ObjectReference::Ref(name) => Some(name),
            _ => None,
        },
        head: space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?,
        ..Default::default()
    };

    // Expected content for each tracked file: the picked version if any,
    // otherwise the last save's.
    let mut tracked: BTreeMap<String, (Hash, Option<PickedChange>)> = BTreeMap::new();
    let head_files = if report.head.is_zero() {
        Default::default()
    } else {
        space.get_commit_files(report.head)?
    };
    for (path, version) in &head_files {
        tracked.insert(path.clone(), (*version, None));
    }
    for path in space.index_paths() {
        if let Some(version) = space.get_indexed_version(&path) {
            let change = if version.is_zero() {
                PickedChange::Deleted
            } else if head_files.contains_key(&path) {
                PickedChange::Modified
            } else {
                PickedChange::New
            };
            tracked.insert(path, (version, Some(change)));
        }
    }

    for (path, (version, picked)) in &tracked {
        if let Some(change) = picked {
            report.picked.push((path.clone(), *change));
            if *change == PickedChange::Deleted {
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
            report.changed.push(path.clone());
        } else if picked.is_none() {
            report.unchanged.push(path.clone());
        }
    }

    let tracked_now = report.tracked();
    report.new = worktree::files(space)?
        .into_iter()
        .filter(|p| tracked_now.binary_search(p).is_err())
        .collect();
    Ok(report)
}
