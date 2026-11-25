//! Choosing what goes into the next save: `duh pick` and `duh unpick`.

use super::look::Look;
use super::{OpResult, Span};
use crate::dedup::DedupProgress;
use crate::diff::DiffFragment;
use crate::file::FileOps;
use crate::hash::Hash;
use crate::space::Space;

/// What picking one file did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    /// Picked, with how it differs from the last save.
    Changed { path: String, version: Hash, strip: Vec<Span> },
    /// Identical to the last save, so nothing was picked.
    Unchanged { path: String },
    /// Its deletion was picked.
    Deletion { path: String },
}

impl Picked {
    pub fn path(&self) -> &str {
        match self {
            Picked::Changed { path, .. } | Picked::Unchanged { path } | Picked::Deletion { path } => path,
        }
    }
}

/// Progress while picking, in order: `Start` for a file, any number of
/// `Chunk`s, then `Done`.
pub enum PickEvent<'a> {
    Start { path: &'a str, size: u64 },
    Chunk(DedupProgress),
    Done(&'a Picked),
}

/// Pick one file by repo path. A file identical to the last save is
/// reported as unchanged and not picked.
pub fn pick_file<F: FnMut(PickEvent)>(space: &mut Space, repo_path: &str, mut on: F) -> OpResult<Picked> {
    let full_path = space.get_path_in_worktree(repo_path)?;
    let size = std::fs::metadata(&full_path).map(|m| m.len()).unwrap_or(0);
    on(PickEvent::Start { path: repo_path, size });

    let mut strip = Vec::new();
    let staged = FileOps::from_space(space).stage_file(
        repo_path.to_string(),
        Some(|fragment: DiffFragment| strip.push(Span::from(&fragment))),
        Some(|event: DedupProgress| on(PickEvent::Chunk(event))),
    )?;
    space.save_index()?;

    let picked = match staged {
        Some(version) => Picked::Changed { path: repo_path.to_string(), version, strip },
        None => Picked::Unchanged { path: repo_path.to_string() },
    };
    on(PickEvent::Done(&picked));
    Ok(picked)
}

/// Pick the deletion of a tracked file.
pub fn pick_deletion(space: &mut Space, repo_path: &str) -> OpResult<Picked> {
    FileOps::from_space(space).stage_deletion(repo_path.to_string())?;
    space.save_index()?;
    Ok(Picked::Deletion { path: repo_path.to_string() })
}

/// Pick every change under `prefix` ("" for the whole space): changed files,
/// deletions of missing files, and (unless `tracked_only`) new files, in
/// that order.
pub fn pick_under<F: FnMut(PickEvent)>(
    space: &mut Space,
    look: &Look,
    prefix: &str,
    tracked_only: bool,
    mut on: F,
) -> OpResult<Vec<Picked>> {
    let under = |p: &String| prefix.is_empty() || p.starts_with(&format!("{}/", prefix));
    let mut out = Vec::new();
    for p in look.changed.iter().filter(|p| under(p)) {
        out.push(pick_file(space, p, &mut on)?);
    }
    for p in look.missing.iter().filter(|p| under(p)) {
        let picked = pick_deletion(space, p)?;
        on(PickEvent::Done(&picked));
        out.push(picked);
    }
    if !tracked_only {
        for p in look.new.iter().filter(|p| under(p)) {
            out.push(pick_file(space, p, &mut on)?);
        }
    }
    Ok(out)
}

/// Take files out of the next save, keeping their edits. Returns each path
/// with whether it had been picked.
pub fn unpick(space: &mut Space, repo_paths: &[String]) -> OpResult<Vec<(String, bool)>> {
    let out = repo_paths
        .iter()
        .map(|p| (p.clone(), space.index.remove(p).is_some()))
        .collect();
    space.save_index()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::{look::look, testing};
    use super::*;

    #[test]
    fn picking_everything_picks_changes_deletions_and_new_files() {
        let (dir, mut space) = testing::space();
        testing::write(&dir, "a.bin", &testing::noise(1, 50_000));
        testing::write(&dir, "b.bin", &testing::noise(2, 50_000));
        let report = look(&mut space).unwrap();
        let picked = pick_under(&mut space, &report, "", false, |_| {}).unwrap();
        assert_eq!(picked.len(), 2);
        crate::ops::save::save(&mut space, "first", false).unwrap();

        testing::write(&dir, "a.bin", &testing::noise(3, 50_000));
        std::fs::remove_file(dir.path().join("b.bin")).unwrap();
        testing::write(&dir, "sub/c.bin", b"new");
        let report = look(&mut space).unwrap();
        assert_eq!(report.changed, vec!["a.bin"]);
        assert_eq!(report.missing, vec!["b.bin"]);
        assert_eq!(report.new, vec!["sub/c.bin"]);

        // Tracked only: no new files.
        let picked = pick_under(&mut space, &report, "", true, |_| {}).unwrap();
        assert!(matches!(&picked[0], Picked::Changed { path, .. } if path == "a.bin"));
        assert_eq!(picked[1], Picked::Deletion { path: "b.bin".into() });
        assert_eq!(picked.len(), 2);

        // The picked set is saved: a fresh open sees it.
        let reopened = crate::ops::open(dir.path()).unwrap();
        assert_eq!(reopened.index.len(), 2);

        assert_eq!(unpick(&mut space, &["a.bin".into(), "zzz".into()]).unwrap(), vec![("a.bin".into(), true), ("zzz".into(), false)]);
    }
}
