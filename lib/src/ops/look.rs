//! What's changed, picked, missing and new: `duh look`.

use std::collections::{BTreeMap, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Hashes of working files by size and modification time, kept in
/// `.duh/statcache`, so looking doesn't re-read files that haven't changed.
/// A file modified within the last couple of seconds isn't trusted to the
/// cache: it could change again within the same timestamp.
struct StatCache {
    entries: HashMap<String, (u64, u128, Hash)>,
    changed: bool,
}

const STAT_CACHE: &str = "statcache";

impl StatCache {
    fn load(space: &Space) -> Self {
        let mut entries = HashMap::new();
        if let Ok(Some(bytes)) = space.read_in_space(STAT_CACHE) {
            for line in String::from_utf8_lossy(&bytes).lines() {
                let mut f = line.splitn(4, '\t');
                if let (Some(h), Some(size), Some(mtime), Some(path)) = (f.next(), f.next(), f.next(), f.next()) {
                    if let (Ok(h), Ok(size), Ok(mtime)) = (Hash::from_string(h.to_string()), size.parse(), mtime.parse()) {
                        entries.insert(path.to_string(), (size, mtime, h));
                    }
                }
            }
        }
        StatCache { entries, changed: false }
    }

    /// The hash of the file at `full` (repo path `path`), from the cache
    /// when its size and time match, else read and remembered.
    fn hash(&mut self, path: &str, full: &std::path::Path) -> OpResult<Hash> {
        let meta = std::fs::metadata(full)?;
        let mtime = meta.modified()?.duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        if let Some((size, t, h)) = self.entries.get(path) {
            if *size == meta.len() && *t == mtime {
                return Ok(*h);
            }
        }
        let h = Hash::digest_file_stream(&mut std::fs::File::open(full)?)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        if now.saturating_sub(mtime) > 2_000_000_000 {
            self.entries.insert(path.to_string(), (meta.len(), mtime, h));
            self.changed = true;
        }
        Ok(h)
    }

    /// Save it if it changed. Best effort: a lost cache only costs a re-read.
    fn save(mut self, space: &Space, live: &[String]) {
        let before = self.entries.len();
        self.entries.retain(|p, _| live.binary_search(p).is_ok());
        if !self.changed && self.entries.len() == before {
            return;
        }
        let mut out = String::new();
        let mut paths: Vec<_> = self.entries.iter().collect();
        paths.sort_by(|a, b| a.0.cmp(b.0));
        for (path, (size, mtime, h)) in paths {
            out.push_str(&format!("{}\t{}\t{}\t{}\n", h.to_hex(), size, mtime, path));
        }
        let _ = space.write_to_space(STAT_CACHE, out.as_bytes());
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

    let mut cache = StatCache::load(space);
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

        let working = cache.hash(path, &full_path)?;
        if content_hash(space, *version)? != Some(working) {
            report.changed.push(path.clone());
        } else if picked.is_none() {
            report.unchanged.push(path.clone());
        }
    }

    let tracked_now = report.tracked();
    cache.save(space, &tracked_now);
    report.new = worktree::files(space)?
        .into_iter()
        .filter(|p| tracked_now.binary_search(p).is_err())
        .collect();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::super::{pick::pick_file, save::save, testing};
    use super::*;

    fn age(path: &std::path::Path, secs: u64) {
        let t = SystemTime::now() - std::time::Duration::from_secs(secs);
        std::fs::File::options().write(true).open(path).unwrap().set_modified(t).unwrap();
    }

    #[test]
    fn look_trusts_the_cache_only_for_files_that_settled() {
        let (dir, mut space) = testing::space();
        let a = dir.path().join("a.bin");
        testing::write(&dir, "a.bin", &testing::noise(1, 20_000));
        pick_file(&mut space, "a.bin", |_| {}).unwrap();
        save(&mut space, "one", false).unwrap();

        // A fresh file isn't cached.
        assert_eq!(look(&mut space).unwrap().unchanged, vec!["a.bin"]);
        assert!(space.read_in_space(STAT_CACHE).unwrap().is_none_or(|b| b.is_empty()));

        // A settled one is, and a later edit is still noticed.
        age(&a, 60);
        look(&mut space).unwrap();
        let cached = String::from_utf8(space.read_in_space(STAT_CACHE).unwrap().unwrap()).unwrap();
        assert!(cached.contains("\ta.bin"));
        testing::write(&dir, "a.bin", &testing::noise(2, 20_000));
        assert_eq!(look(&mut space).unwrap().changed, vec!["a.bin"]);

        // Same size, back-dated to the cached time: the cache would be
        // fooled, which is the trade git makes too; restoring the real
        // content brings it back to unchanged.
        testing::write(&dir, "a.bin", &testing::noise(1, 20_000));
        assert_eq!(look(&mut space).unwrap().unchanged, vec!["a.bin"]);
    }
}
