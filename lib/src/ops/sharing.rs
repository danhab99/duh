//! Sharing with places: `duh get`, `duh send`, `duh receive`, and how far
//! apart you and a place are.

use std::fs;
use std::path::Path;

use super::look::look;
use super::{worktree, OpResult};
use crate::hash::Hash;
use crate::objects::{Object, ObjectReference};
use crate::space::Space;
use crate::transfer::TransferStats;

/// What a send or receive did.
#[derive(Clone, Debug)]
pub struct Transfer {
    pub place: String,
    /// The draft updated (on the place for a send, here for a receive).
    pub draft: String,
    /// The draft it came from (here for a send, on the place for a receive).
    pub source_draft: String,
    /// Where the updated draft was before (zero: it didn't exist).
    pub before: Hash,
    pub after: Hash,
    pub stats: TransferStats,
}

/// How a draft here compares with one on a place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncStatus {
    /// Saves here that the place doesn't have.
    pub to_send: usize,
    /// Saves on the place that you don't have.
    pub to_receive: usize,
    /// Whether the place has the draft at all.
    pub place_has_draft: bool,
}

impl SyncStatus {
    /// Both sides have saves the other lacks: sending or receiving would
    /// drop someone's work, and duh doesn't merge.
    pub fn diverged(&self) -> bool {
        self.to_send > 0 && self.to_receive > 0
    }
}

fn open_draft_name(space: &Space) -> String {
    space.get_head_branch_name().unwrap_or_else(|_| "HEAD".to_string())
}

/// Copy the space at `url` into a new folder `dest` (removed again if
/// anything fails), opening `draft` (default: the one the space is on).
/// `options` are storage settings saved for the place, e.g. credentials.
pub fn get(url: &str, dest: &Path, draft: Option<&str>, bare: bool, options: &[(String, String)]) -> OpResult<()> {
    if dest.exists() {
        return Err(format!("folder '{}' already exists", dest.display()).into());
    }
    let metadata = dest.join(crate::utils::SPACE_METADATA_DIR_NAME);
    fs::create_dir_all(&metadata)?;
    get_into(url, dest, &metadata, draft, bare, options).inspect_err(|_| {
        let _ = fs::remove_dir_all(dest);
    })
}

fn get_into(url: &str, dest: &Path, metadata: &Path, draft: Option<&str>, bare: bool, options: &[(String, String)]) -> OpResult<()> {
    let worktree = if bare { None } else { Some(dest.to_path_buf()) };
    let mut space = Space::init(crate::store::local(metadata)?, worktree)?;
    space.add_remote_with_options("origin", url, options)?;
    let remote = space.get_remote_by_name("origin")?;

    let head_name = remote.get_head_branch_name().unwrap_or_else(|_| "HEAD".to_string());
    let target = draft.unwrap_or(&head_name);
    // An empty space has no drafts yet.
    let remote_hash = match remote.resolve_ref_name(ObjectReference::Ref(target.to_string())) {
        Ok(h) => h,
        Err(_) if draft.is_some() => return Err(format!("that space has no draft '{}'", target).into()),
        Err(_) => Hash::new(),
    };
    remote.copy_commit_to(&space, remote_hash)?;

    space.set_ref(target, ObjectReference::Hash(remote_hash), Some(&format!("clone from origin: {}", target)))?;
    space.set_ref("HEAD", ObjectReference::Ref(target.to_string()), Some("get"))?;
    if !bare {
        for path in space.list_files(ObjectReference::Hash(remote_hash))? {
            worktree::write_version(&mut space, &path, remote_hash, None)?;
        }
    }
    space.save_index()?;
    Ok(())
}

/// Send your saves to `place`'s `draft` (default: the same name as the one
/// you're on). Only ever moves the place's draft forward, unless `force`.
pub fn send(space: &mut Space, place: &str, draft: Option<&str>, force: bool) -> OpResult<Transfer> {
    let source_draft = open_draft_name(space);
    let target = draft.unwrap_or(&source_draft).to_string();
    let remote = space.get_or_create_remote(place)?;
    let head = space.get_head_commit_hash()?;
    if head.is_zero() {
        return Err("nothing to send: no saves yet".into());
    }

    // Like git, don't move the draft a space with working files has open:
    // its files would no longer match.
    let remote_is_bare = remote.get_config_value("core.bare").map(|v| v == "true").unwrap_or(true);
    if !remote_is_bare && remote.get_head_branch_name().ok().as_deref() == Some(target.as_str()) {
        return Err(format!(
            "won't update '{}': it's the draft open in {}, whose files would go stale; send to another draft or a bare space",
            target, place
        )
        .into());
    }

    let before = remote.resolve_ref_name(ObjectReference::Ref(target.clone())).unwrap_or_default();
    if !force && !space.is_ancestor(before, head)? {
        return Err(format!(
            "{}/{} has saves you don't; `duh receive {}` first (or --force to discard them)",
            place, target, place
        )
        .into());
    }

    let stats = space.copy_commit_to(&remote, head)?;
    remote.set_ref(&target, ObjectReference::Hash(head), Some(&format!("send: {}", target)))?;
    Ok(Transfer { place: place.to_string(), draft: target, source_draft, before, after: head, stats })
}

/// Receive `place`'s `draft` (default: the same name as `into`) into your
/// draft `into` (default: the one you're on), moving it forward only. When
/// `into` is open, your files are updated to match.
pub fn receive(space: &mut Space, place: &str, draft: Option<&str>, into: Option<&str>) -> OpResult<Transfer> {
    let open = open_draft_name(space);
    let local = into.unwrap_or(&open).to_string();
    let remote_draft = draft.unwrap_or(&local).to_string();
    let remote = space.get_remote_by_name(place)?;
    let after = remote
        .resolve_ref_name(ObjectReference::Ref(remote_draft.clone()))
        .map_err(|_| format!("{} has no draft '{}'", place, remote_draft))?;

    let is_open = space.get_ref("HEAD".into())? == ObjectReference::Ref(local.clone());
    if is_open && space.worktree.is_some() && look(space)?.is_dirty() {
        return Err("you have unsaved changes; save or bring back your files before receiving".into());
    }

    let before = space.resolve_ref_name(ObjectReference::Ref(local.clone())).unwrap_or_default();
    let stats = remote.copy_commit_to(space, after)?;
    if !space.is_ancestor(before, after)? {
        return Err(format!(
            "your draft '{}' has saves that {}/{} doesn't; duh doesn't merge, so it only receives when nothing would be lost",
            local, place, remote_draft
        )
        .into());
    }

    space.set_ref(&local, ObjectReference::Hash(after), Some(&format!("receive from {}/{}", place, remote_draft)))?;
    if is_open && space.worktree.is_some() {
        worktree::match_version(space, before, after)?;
    }
    Ok(Transfer { place: place.to_string(), draft: local, source_draft: remote_draft, before, after, stats })
}

/// The saves reachable from `from`, newest first, read from `space`.
fn ancestry(space: &Space, from: Hash) -> OpResult<Vec<Hash>> {
    let mut out = Vec::new();
    let mut cur = from;
    while !cur.is_zero() {
        out.push(cur);
        cur = match space.get_object(cur)? {
            Some(Object::Commit(c)) => c.parent,
            _ => break,
        };
    }
    Ok(out)
}

/// How your draft (default: the one you're on) compares with the draft of
/// the same name on `place`. Reads the place, so it can take a moment.
pub fn sync_status(space: &Space, place: &str, draft: Option<&str>) -> OpResult<SyncStatus> {
    let open = open_draft_name(space);
    let name = draft.unwrap_or(&open);
    let remote = space.get_remote_by_name(place)?;
    let here = space.resolve_ref_name(ObjectReference::Ref(name.to_string())).unwrap_or_default();
    // A new space's first draft exists but points at nothing yet.
    let there = remote.resolve_ref_name(ObjectReference::Ref(name.to_string())).unwrap_or_default();
    let place_has_draft = !there.is_zero();

    // Histories are linear, so each side is a list; what they share is a
    // common tail. Saves the place has that we don't are read from the place.
    let mine = ancestry(space, here)?;
    let theirs = ancestry(&remote, there)?;
    let mine_set: std::collections::HashSet<_> = mine.iter().collect();
    let theirs_set: std::collections::HashSet<_> = theirs.iter().collect();
    Ok(SyncStatus {
        to_send: mine.iter().filter(|h| !theirs_set.contains(h)).count(),
        to_receive: theirs.iter().filter(|h| !mine_set.contains(h)).count(),
        place_has_draft,
    })
}

#[cfg(test)]
mod tests {
    use super::super::{pick::pick_file, save::save, testing};
    use super::*;

    fn save_text(dir: &tempfile::TempDir, space: &mut Space, text: &str) -> Hash {
        testing::write(dir, "a.txt", text.as_bytes());
        pick_file(space, "a.txt", |_| {}).unwrap();
        save(space, text, false).unwrap().version
    }

    #[test]
    fn send_receive_and_sync_status() {
        let (dir, mut space) = testing::space();
        let hub = tempfile::TempDir::new().unwrap();
        let hub_meta = hub.path().join(".duh");
        fs::create_dir_all(&hub_meta).unwrap();
        Space::init(crate::store::local(&hub_meta).unwrap(), None).unwrap();
        space.add_remote("origin", hub.path().to_str().unwrap()).unwrap();

        save_text(&dir, &mut space, "one");
        let s = sync_status(&space, "origin", None).unwrap();
        assert_eq!((s.to_send, s.to_receive, s.place_has_draft), (1, 0, false));
        let sent = send(&mut space, "origin", None, false).unwrap();
        assert_eq!(sent.draft, "main");
        assert_eq!(sync_status(&space, "origin", None).unwrap().to_send, 0);

        // A teammate gets a copy, saves and sends.
        let mate_dir = tempfile::TempDir::new().unwrap();
        let mate_path = mate_dir.path().join("mate");
        get(hub.path().to_str().unwrap(), &mate_path, None, false, &[]).unwrap();
        let mut mate = crate::ops::open(&mate_path).unwrap();
        fs::write(mate_path.join("a.txt"), "two").unwrap();
        pick_file(&mut mate, "a.txt", |_| {}).unwrap();
        save(&mut mate, "two", false).unwrap();
        send(&mut mate, "origin", None, false).unwrap();

        // Meanwhile we saved too: diverged.
        save_text(&dir, &mut space, "mine");
        let s = sync_status(&space, "origin", None).unwrap();
        assert!(s.diverged() && s.to_send == 1 && s.to_receive == 1);
        assert!(send(&mut space, "origin", None, false).unwrap_err().to_string().contains("has saves you don't"));
        assert!(receive(&mut space, "origin", None, None).unwrap_err().to_string().contains("doesn't merge"));

        // Receiving into a new draft keeps both.
        let got = receive(&mut space, "origin", Some("main"), Some("theirs")).unwrap();
        assert_eq!((got.draft.as_str(), got.before), ("theirs", Hash::new()));
        assert!(get(hub.path().to_str().unwrap(), &mate_path, None, false, &[]).unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn the_first_send_to_an_empty_folder_starts_a_space_there() {
        let (dir, mut space) = testing::space();
        save_text(&dir, &mut space, "one");

        // An empty folder, like a fresh USB drive, and one that doesn't exist yet.
        let usb = tempfile::TempDir::new().unwrap();
        let later = usb.path().join("not-yet");
        space.add_remote("usb", usb.path().to_str().unwrap()).unwrap();
        space.add_remote("later", later.to_str().unwrap()).unwrap();
        send(&mut space, "usb", None, false).unwrap();
        send(&mut space, "later", None, false).unwrap();

        // Both now hold a space a teammate can get a copy of.
        for from in [usb.path(), later.as_path()] {
            let to = tempfile::TempDir::new().unwrap();
            get(from.to_str().unwrap(), &to.path().join("copy"), None, false, &[]).unwrap();
            assert_eq!(fs::read_to_string(to.path().join("copy/a.txt")).unwrap(), "one");
        }

        // A folder that holds other files is left alone.
        let busy = tempfile::TempDir::new().unwrap();
        fs::write(busy.path().join("holiday.jpg"), "photo").unwrap();
        space.add_remote("busy", busy.path().to_str().unwrap()).unwrap();
        let err = send(&mut space, "busy", None, false).unwrap_err().to_string();
        assert!(err.contains("holds files but no duh space"), "{err}");
        assert_eq!(fs::read_dir(busy.path()).unwrap().count(), 1);

        // Getting a copy from a folder with no space says so.
        let to = tempfile::TempDir::new().unwrap();
        let err = get(busy.path().to_str().unwrap(), &to.path().join("copy"), None, false, &[]).unwrap_err().to_string();
        assert!(err.contains("no duh space") || err.contains("not a duh space") || err.contains("missing config"), "{err}");
    }
}
