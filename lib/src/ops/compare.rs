//! How much of each file changed: `duh compare`.

use std::fs;

use super::look::{Look, PickedChange};
use super::{OpResult, Span};
use crate::dedup::{build_diff_fragments, DedupProgress};
use crate::file::FileOps;
use crate::objects::Object;
use crate::space::{ReadSeek, Space};

/// What kind of difference a comparison is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difference {
    Changed,
    New,
    Deleted,
    /// Tracked but gone from the folder.
    Missing,
}

/// One file's difference, with its change strip (empty for deletions and
/// missing files).
#[derive(Clone, Debug)]
pub struct Comparison {
    pub path: String,
    pub difference: Difference,
    pub strip: Vec<Span>,
}

/// The strip from `old` to `new`, computed in memory (nothing is stored).
pub fn strip_between(space: &Space, old: Box<dyn ReadSeek>, new: Box<dyn ReadSeek>) -> OpResult<Vec<Span>> {
    let frags = build_diff_fragments(
        old,
        new,
        space.chunk_size,
        space.chunk_size as u64,
        space.max_chunk(),
        None::<fn(DedupProgress)>,
    )?;
    Ok(frags.map(|f| Span::from(&f)).collect())
}

/// With `picked`, what's picked against the last save; otherwise your
/// un-picked edits against what's picked (or the last save). `only` limits
/// it to those repo paths (empty: everything).
pub fn compare(space: &mut Space, look: &Look, only: &[String], picked: bool) -> OpResult<Vec<Comparison>> {
    let wanted = |p: &String| only.is_empty() || only.contains(p);
    let mut out = Vec::new();

    if picked {
        for (path, change) in look.picked.iter().filter(|(p, _)| wanted(p)) {
            if *change == PickedChange::Deleted {
                out.push(Comparison { path: path.clone(), difference: Difference::Deleted, strip: vec![] });
                continue;
            }
            let version = match space.get_indexed_version(path).map(|h| space.get_object(h)) {
                Some(Ok(Some(Object::File(f)))) => f,
                _ => return Err(format!("picked version of '{}' is unreadable", path).into()),
            };
            let strip = FileOps::from_space(space).load_fragments(&version)?.iter().map(Span::from).collect();
            let difference = if *change == PickedChange::New { Difference::New } else { Difference::Changed };
            out.push(Comparison { path: path.clone(), difference, strip });
        }
        return Ok(out);
    }

    let head = space.get_head_commit_hash()?;
    for path in look.changed.iter().filter(|p| wanted(p)) {
        let is_picked = space.index.get(path).is_some_and(|h| !h.is_zero());
        let old = if is_picked {
            FileOps::from_space(space).open_staged(path.clone())?
        } else {
            FileOps::from_space(space).open_file(path.clone(), head)?
        };
        let new: Box<dyn ReadSeek> = Box::new(fs::File::open(space.get_path_in_worktree(path)?)?);
        let strip = strip_between(space, old, new)?;
        out.push(Comparison { path: path.clone(), difference: Difference::Changed, strip });
    }
    for path in look.missing.iter().filter(|p| wanted(p)) {
        out.push(Comparison { path: path.clone(), difference: Difference::Missing, strip: vec![] });
    }
    Ok(out)
}
