//! What the bridge does, in plain Rust so it can be tested without Dart.
//! Every function opens the space at `folder` through `lib::ops`; those that
//! change it hold the space's writer lock while they work.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Duration;

use lib::objects::{Object, ObjectReference};
use lib::ops::{self, look::PickedChange, Span};
use lib::space::Space;

use crate::api::types::*;

type R<T> = anyhow::Result<T>;

/// lib's errors are already sentences; carry them over as they are.
fn ok<T>(r: Result<T, Box<dyn Error>>) -> R<T> {
    r.map_err(|e| anyhow::anyhow!(e.to_string()))
}

fn open(folder: &str) -> R<Space> {
    ok(ops::open(Path::new(folder)))
}

fn lock(folder: &str) -> R<ops::WriteLock> {
    ok(ops::lock(Path::new(folder), Duration::from_secs(10)))
}

fn version_ref(v: &str) -> ObjectReference {
    ObjectReference::from(v.to_string())
}

fn hex(h: lib::hash::Hash) -> String {
    if h.is_zero() { String::new() } else { h.to_hex() }
}

fn segments(strip: &[Span]) -> Vec<Segment> {
    strip
        .iter()
        .map(|s| match *s {
            Span::Kept(n) => Segment { kind: SegmentKind::Kept, len: n as i64 },
            Span::Added(n) => Segment { kind: SegmentKind::Added, len: n as i64 },
            Span::Removed(n) => Segment { kind: SegmentKind::Removed, len: n as i64 },
        })
        .collect()
}

fn parse_options(options: &[String]) -> R<Vec<(String, String)>> {
    options.iter().map(|o| ok(lib::store::parse_option(o))).collect()
}

fn info_of(space: &Space, folder: &str) -> SpaceInfo {
    let root: PathBuf = space.worktree.clone().unwrap_or_else(|| PathBuf::from(folder));
    SpaceInfo {
        name: root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        root: root.to_string_lossy().to_string(),
        draft: space.get_head_branch_name().ok(),
    }
}

// Spaces

pub fn open_space(folder: &str) -> R<SpaceInfo> {
    let space = open(folder)?;
    Ok(info_of(&space, folder))
}

pub fn start_space(folder: &str) -> R<SpaceInfo> {
    let root = Path::new(folder);
    let meta = root.join(lib::utils::SPACE_METADATA_DIR_NAME);
    if meta.exists() {
        anyhow::bail!("there's already a space in {}", folder);
    }
    std::fs::create_dir_all(&meta)?;
    let space = ok(Space::init(ok(lib::store::local(&meta))?, Some(root.to_path_buf())))?;
    Ok(info_of(&space, folder))
}

/// Start a bare space (history only, for sharing) in `folder`, which must
/// be empty or not exist yet.
pub fn start_bare_space(folder: &str) -> R<()> {
    let root = Path::new(folder);
    if root.exists() && std::fs::read_dir(root)?.next().is_some() {
        anyhow::bail!("{} isn't empty; a place needs an empty folder or an existing space", folder);
    }
    let meta = root.join(lib::utils::SPACE_METADATA_DIR_NAME);
    std::fs::create_dir_all(&meta)?;
    ok(Space::init(ok(lib::store::local(&meta))?, None))?;
    Ok(())
}

pub fn get_space(url: &str, dest: &str, options: &[String]) -> R<SpaceInfo> {
    ok(ops::sharing::get(url, Path::new(dest), None, false, &parse_options(options)?))?;
    open_space(dest)
}

// Your files

pub fn look(folder: &str) -> R<LookResult> {
    let mut space = open(folder)?;
    let report = ok(ops::look::look(&mut space))?;
    let mut files: Vec<WorkFile> = Vec::new();
    let size = |space: &Space, p: &str| -> i64 {
        space.get_path_in_worktree(p).ok().and_then(|f| std::fs::metadata(f).ok()).map(|m| m.len() as i64).unwrap_or(0)
    };
    let mut push = |path: &String, state: WorkState, picked: PickedKind| {
        files.push(WorkFile { size: size(&space, path), path: path.clone(), state, picked });
    };
    for (p, change) in &report.picked {
        let kind = match change {
            PickedChange::New => PickedKind::New,
            PickedChange::Modified => PickedKind::Modified,
            PickedChange::Deleted => PickedKind::Deleted,
        };
        let state = if report.changed.contains(p) { WorkState::Changed } else { WorkState::Picked };
        push(p, state, kind);
    }
    let picked: Vec<&String> = report.picked.iter().map(|(p, _)| p).collect();
    for p in report.changed.iter().filter(|p| !picked.contains(p)) {
        push(p, WorkState::Changed, PickedKind::None);
    }
    for p in &report.missing {
        push(p, WorkState::Missing, PickedKind::None);
    }
    for p in &report.new {
        push(p, WorkState::New, PickedKind::None);
    }
    for p in &report.unchanged {
        push(p, WorkState::Unchanged, PickedKind::None);
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(LookResult { draft: report.draft, head: hex(report.head), files })
}

pub fn compare_file(folder: &str, path: &str) -> R<Vec<Segment>> {
    let mut space = open(folder)?;
    let report = ok(ops::look::look(&mut space))?;
    let only = [path.to_string()];
    if report.new.iter().any(|p| p == path) {
        let len = std::fs::metadata(ok(space.get_path_in_worktree(path))?)?.len();
        return Ok(vec![Segment { kind: SegmentKind::Added, len: len as i64 }]);
    }
    let picked_only = !report.changed.iter().any(|p| p == path);
    let found = ok(ops::compare::compare(&mut space, &report, &only, picked_only))?;
    Ok(found.first().map(|c| segments(&c.strip)).unwrap_or_default())
}

pub fn pick(folder: &str, paths: &[String], mut on: impl FnMut(Progress)) -> R<Vec<Picked>> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    let report = ok(ops::look::look(&mut space))?;
    let mut out = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        on(Progress { path: p.clone(), done: i as u32, total: paths.len() as u32 });
        let picked = if report.missing.contains(p) {
            ok(ops::pick::pick_deletion(&mut space, p))?
        } else {
            ok(ops::pick::pick_file(&mut space, p, |_| {}))?
        };
        out.push(match picked {
            ops::pick::Picked::Changed { strip, .. } => Picked { path: p.clone(), changed: true, deleted: false, strip: segments(&strip) },
            ops::pick::Picked::Unchanged { .. } => Picked { path: p.clone(), changed: false, deleted: false, strip: vec![] },
            ops::pick::Picked::Deletion { .. } => Picked { path: p.clone(), changed: true, deleted: true, strip: vec![] },
        });
    }
    on(Progress { path: String::new(), done: paths.len() as u32, total: paths.len() as u32 });
    Ok(out)
}

pub fn unpick(folder: &str, paths: &[String]) -> R<()> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    ok(ops::pick::unpick(&mut space, paths))?;
    Ok(())
}

pub fn suggest_message(folder: &str) -> R<String> {
    let mut space = open(folder)?;
    ok(ops::save::suggest_message(&mut space, |p| p.to_string()))
}

pub fn save(folder: &str, message: &str) -> R<Saved> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    let saved = ok(ops::save::save(&mut space, message, false))?;
    Ok(Saved { version: hex(saved.version), draft: saved.draft })
}

pub fn bring_back(folder: &str, path: &str, from: Option<&str>, as_path: Option<&str>) -> R<()> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    let from = from.map(version_ref);
    ok(ops::files::bring_back(&mut space, &[path.to_string()], from.as_ref(), as_path.map(Path::new)))?;
    Ok(())
}

// History

/// Bytes added and removed, and files changed, by a save: read from the
/// stored changes, without rebuilding any file.
fn change_summary(space: &mut Space, version: lib::hash::Hash, previous: lib::hash::Hash) -> R<(i64, i64, u32)> {
    let now = ok(space.get_commit_files(version))?;
    let before = if previous.is_zero() { Default::default() } else { ok(space.get_commit_files(previous))? };
    let (mut added, mut removed, mut files) = (0i64, 0i64, 0u32);
    for (path, v) in &now {
        if before.get(path) == Some(v) {
            continue;
        }
        files += 1;
        if let Ok(Some(Object::File(f))) = space.get_object(*v) {
            let frags = ok(lib::file::FileOps::from_space(space).load_fragments(&f))?;
            let strip: Vec<Span> = frags.iter().map(Span::from).collect();
            let (a, r, _) = ops::strip_totals(&strip);
            added += a as i64;
            removed += r as i64;
        }
    }
    files += before.keys().filter(|p| !now.contains_key(*p)).count() as u32;
    Ok((added, removed, files))
}

fn save_info(space: &mut Space, s: ops::history::SaveInfo) -> R<SaveInfo> {
    let (added, removed, files_changed) = change_summary(space, s.version, s.previous)?;
    Ok(SaveInfo {
        version: hex(s.version),
        previous: hex(s.previous),
        message: s.message,
        author: s.author_name,
        time: s.time as i64,
        labels: s.labels,
        drafts: s.drafts,
        added,
        removed,
        files_changed,
    })
}

pub fn history(folder: &str, max: Option<u32>) -> R<Vec<SaveInfo>> {
    let mut space = open(folder)?;
    let head = ok(space.resolve_ref_name(ObjectReference::Ref("HEAD".into())))?;
    let saves = ok(ops::history::history(&space, head, max.map(|m| m as usize)))?;
    saves.into_iter().map(|s| save_info(&mut space, s)).collect()
}

pub fn inspect(folder: &str, version: &str) -> R<SaveDetail> {
    let mut space = open(folder)?;
    let v = ok(space.resolve_ref_name(version_ref(version)))?;
    let d = ok(ops::history::inspect(&mut space, v))?;
    let files = d
        .files
        .iter()
        .map(|f| FileInSave { path: f.path.clone(), size: f.size as i64, changed: f.changed, strip: segments(&f.strip) })
        .collect();
    Ok(SaveDetail { info: save_info(&mut space, d.info)?, files })
}

pub fn journal(folder: &str, draft: &str) -> R<Vec<JournalItem>> {
    let space = open(folder)?;
    Ok(ok(ops::history::journal(&space, draft))?
        .into_iter()
        .map(|e| JournalItem { version: hex(e.version), what: e.what })
        .collect())
}

// Drafts and labels

pub fn drafts(folder: &str) -> R<Vec<DraftInfo>> {
    let space = open(folder)?;
    Ok(ok(ops::refs::drafts(&space))?
        .into_iter()
        .map(|d| DraftInfo { name: d.name, version: hex(d.version), open: d.open })
        .collect())
}

pub fn start_draft(folder: &str, name: &str, at: Option<&str>) -> R<()> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    let at = at.map(version_ref);
    ok(ops::refs::start_draft(&mut space, name, at.as_ref()))?;
    Ok(())
}

pub fn open_draft(folder: &str, name: &str) -> R<()> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    ok(ops::refs::open_draft(&mut space, name, false))
}

pub fn delete_draft(folder: &str, name: &str) -> R<()> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    ok(ops::refs::delete_draft(&mut space, name))
}

pub fn rename_draft(folder: &str, new_name: &str) -> R<()> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    ok(ops::refs::rename_draft(&mut space, new_name))
}

pub fn labels(folder: &str) -> R<Vec<LabelInfo>> {
    let space = open(folder)?;
    Ok(ok(ops::refs::labels(&space))?.into_iter().map(|l| LabelInfo { name: l.name, version: hex(l.version) }).collect())
}

pub fn add_label(folder: &str, name: &str, version: &str) -> R<()> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    ok(ops::refs::add_label(&mut space, name, Some(&version_ref(version))))?;
    Ok(())
}

pub fn remove_label(folder: &str, name: &str) -> R<()> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    ok(ops::refs::remove_label(&mut space, name))
}

pub fn rewind(folder: &str, version: &str, mode: RewindMode) -> R<()> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    let mode = match mode {
        RewindMode::KeepPicked => ops::refs::Rewind::KeepPicked,
        RewindMode::KeepFiles => ops::refs::Rewind::KeepFiles,
        RewindMode::Discard => ops::refs::Rewind::Discard,
    };
    ok(ops::refs::rewind(&mut space, Some(&version_ref(version)), mode))?;
    Ok(())
}

// Places

pub fn places(folder: &str) -> R<Vec<PlaceInfo>> {
    let space = open(folder)?;
    Ok(ok(space.list_remotes())?.into_iter().map(|(name, url)| PlaceInfo { name, url }).collect())
}

pub fn add_place(folder: &str, name: &str, url: &str, options: &[String]) -> R<()> {
    let _lock = lock(folder)?;
    let space = open(folder)?;
    ok(space.add_remote_with_options(name, url, &parse_options(options)?))
}

pub fn remove_place(folder: &str, name: &str) -> R<()> {
    let _lock = lock(folder)?;
    let space = open(folder)?;
    ok(space.remove_remote(name))
}

pub fn sync_status(folder: &str, place: &str) -> R<SyncInfo> {
    let space = open(folder)?;
    let s = ok(ops::sharing::sync_status(&space, place, None))?;
    Ok(SyncInfo { to_send: s.to_send as u32, to_receive: s.to_receive as u32, place_has_draft: s.place_has_draft, diverged: s.diverged() })
}

fn transfer(t: ops::sharing::Transfer) -> TransferResult {
    TransferResult { draft: t.draft, saves: t.saves as u32, bytes: t.stats.bytes as i64 }
}

pub fn send(folder: &str, place: &str, draft: Option<&str>, force: bool) -> R<TransferResult> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    Ok(transfer(ok(ops::sharing::send(&mut space, place, draft, force))?))
}

pub fn receive(folder: &str, place: &str, draft: Option<&str>, into: Option<&str>) -> R<TransferResult> {
    let _lock = lock(folder)?;
    let mut space = open(folder)?;
    Ok(transfer(ok(ops::sharing::receive(&mut space, place, draft, into))?))
}

// Health

pub fn check(folder: &str, place: Option<&str>) -> R<CheckResult> {
    let space = open(folder)?;
    let mut target = match place {
        Some(p) => ok(space.get_remote_by_name(p))?,
        None => space,
    };
    let r = ok(target.check(|_| {}))?;
    Ok(CheckResult {
        objects: r.objects as u32,
        saves: r.saves as u32,
        file_versions: r.file_versions as u32,
        solids: r.solids as u32,
        problems: r.problems,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole loop the app drives: start, pick, save, history, inspect,
    /// drafts, labels, bring back, a place, send, check.
    #[test]
    fn the_whole_loop() {
        let dir = tempfile::TempDir::new().unwrap();
        let folder = dir.path().to_str().unwrap();
        let info = start_space(folder).unwrap();
        assert_eq!(info.draft.as_deref(), Some("main"));
        assert!(start_space(folder).is_err());
        assert!(start_bare_space(folder).is_err()); // not empty

        std::fs::write(dir.path().join("a.bin"), vec![7u8; 50_000]).unwrap();
        let l = look(folder).unwrap();
        assert_eq!((l.files.len(), l.files[0].state, l.head.as_str()), (1, WorkState::New, ""));
        assert_eq!(compare_file(folder, "a.bin").unwrap(), vec![Segment { kind: SegmentKind::Added, len: 50_000 }]);

        let mut progress = Vec::new();
        let picked = pick(folder, &["a.bin".into()], |p| progress.push(p.done)).unwrap();
        assert!(picked[0].changed);
        assert_eq!(progress, [0, 1]);
        assert_eq!(look(folder).unwrap().files[0].state, WorkState::Picked);
        assert!(suggest_message(folder).unwrap().starts_with("Update 1 file"));
        let first = save(folder, "first").unwrap();

        std::fs::write(dir.path().join("a.bin"), vec![8u8; 50_000]).unwrap();
        assert_eq!(look(folder).unwrap().files[0].state, WorkState::Changed);
        assert!(!compare_file(folder, "a.bin").unwrap().is_empty());
        pick(folder, &["a.bin".into()], |_| {}).unwrap();
        save(folder, "second").unwrap();

        let h = history(folder, None).unwrap();
        assert_eq!(h.iter().map(|s| s.message.as_str()).collect::<Vec<_>>(), ["second", "first"]);
        assert_eq!((h[1].files_changed, h[1].added), (1, 50_000));
        assert_eq!(h[0].drafts, ["main"]);
        let d = inspect(folder, &first.version).unwrap();
        assert_eq!((d.files[0].size, d.files[0].changed), (50_000, true));

        add_label(folder, "v1", &first.version).unwrap();
        assert_eq!(labels(folder).unwrap()[0].version, first.version);
        start_draft(folder, "alt", Some(&first.version)).unwrap();
        open_draft(folder, "alt").unwrap();
        assert_eq!(std::fs::read(dir.path().join("a.bin")).unwrap(), vec![7u8; 50_000]);
        assert!(drafts(folder).unwrap().iter().any(|d| d.name == "alt" && d.open));
        open_draft(folder, "main").unwrap();

        let copy = dir.path().join("copy.bin");
        bring_back(folder, "a.bin", Some("v1"), Some(copy.to_str().unwrap())).unwrap();
        assert_eq!(std::fs::read(&copy).unwrap(), vec![7u8; 50_000]);
        std::fs::remove_file(&copy).unwrap();

        let hub = tempfile::TempDir::new().unwrap();
        start_bare_space(hub.path().to_str().unwrap()).unwrap();
        add_place(folder, "origin", hub.path().to_str().unwrap(), &[]).unwrap();
        assert_eq!(places(folder).unwrap()[0].name, "origin");
        let s = sync_status(folder, "origin").unwrap();
        assert_eq!((s.to_send, s.to_receive, s.diverged), (2, 0, false));
        assert_eq!(send(folder, "origin", None, false).unwrap().saves, 2);
        assert_eq!(sync_status(folder, "origin").unwrap().to_send, 0);

        let c = check(folder, None).unwrap();
        assert!(c.problems.is_empty() && c.saves == 2);
        assert!(check(folder, Some("origin")).unwrap().problems.is_empty());
        assert!(!journal(folder, "main").unwrap().is_empty());

        rewind(folder, &first.version, RewindMode::Discard).unwrap();
        assert_eq!(history(folder, None).unwrap().len(), 1);
    }
}
