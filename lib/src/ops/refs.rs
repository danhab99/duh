//! Drafts, labels and rewinding: `duh drafts`, `duh open`, `duh label`,
//! `duh rewind`.

use super::look::look;
use super::{worktree, OpResult};
use crate::file::FileOps;
use crate::hash::Hash;
use crate::objects::{Object, ObjectReference};
use crate::space::Space;

/// A draft and the save it's at.
#[derive(Clone, Debug)]
pub struct Draft {
    pub name: String,
    pub version: Hash,
    /// The draft that's open.
    pub open: bool,
}

/// A label and the save it names.
#[derive(Clone, Debug)]
pub struct Label {
    pub name: String,
    pub version: Hash,
}

fn head_ref(space: &Space) -> OpResult<ObjectReference> {
    space.get_ref("HEAD".into())
}

fn resolve_or_head(space: &Space, r: Option<&ObjectReference>) -> OpResult<Hash> {
    space.resolve_ref_name(r.cloned().unwrap_or(ObjectReference::Ref("HEAD".into())))
}

pub fn drafts(space: &Space) -> OpResult<Vec<Draft>> {
    let head = head_ref(space)?;
    let mut out = Vec::new();
    for name in space.list_branches()? {
        let version = space.resolve_ref_name(ObjectReference::Ref(name.clone())).unwrap_or_default();
        out.push(Draft { open: head == ObjectReference::Ref(name.clone()), name, version });
    }
    Ok(out)
}

/// Start a draft at `start` (default: where you are). Doesn't open it.
pub fn start_draft(space: &mut Space, name: &str, start: Option<&ObjectReference>) -> OpResult<Hash> {
    if space.list_branches()?.iter().any(|b| b == name) {
        return Err(format!("there's already a draft called '{}'", name).into());
    }
    let commit = resolve_or_head(space, start)?;
    if commit.is_zero() {
        return Err("save something first; a draft starts from a version".into());
    }
    space.set_ref(name, ObjectReference::Hash(commit), Some("draft: created"))?;
    Ok(commit)
}

pub fn delete_draft(space: &mut Space, name: &str) -> OpResult<()> {
    if head_ref(space)? == ObjectReference::Ref(name.to_string()) {
        return Err(format!("can't delete '{}': it's the draft you have open", name).into());
    }
    if !space.list_branches()?.iter().any(|b| b == name) {
        return Err(format!("there's no draft called '{}'", name).into());
    }
    space.delete_ref(name)
}

/// Rename the open draft.
pub fn rename_draft(space: &mut Space, new_name: &str) -> OpResult<()> {
    let ObjectReference::Ref(current) = head_ref(space)? else {
        return Err("you're not on a draft, so there's nothing to rename".into());
    };
    if space.list_branches()?.iter().any(|b| b == new_name) {
        return Err(format!("there's already a draft called '{}'", new_name).into());
    }
    let at = space.resolve_ref_name(ObjectReference::Ref(current.clone()))?;
    space.delete_ref(&current)?;
    space.set_ref(new_name, ObjectReference::Hash(at), Some("draft: renamed"))?;
    space.set_ref("HEAD", ObjectReference::Ref(new_name.to_string()), Some("draft: renamed"))?;
    Ok(())
}

/// Point the open draft at another version (without touching your files).
pub fn move_draft(space: &mut Space, to: &ObjectReference) -> OpResult<Hash> {
    let ObjectReference::Ref(current) = head_ref(space)? else {
        return Err("you're not on a draft; open one first".into());
    };
    let target = space.resolve_ref_name(to.clone())?;
    if !matches!(space.get_object(target)?, Some(Object::Commit(_))) {
        return Err(format!("'{}' isn't a version", to).into());
    }
    space.set_ref(&current, ObjectReference::Hash(target), Some("draft: moved"))?;
    Ok(target)
}

/// Open a draft (with `create`, start it where you are first), making your
/// files match it. Refuses when you have unsaved changes.
pub fn open_draft(space: &mut Space, name: &str, create: bool) -> OpResult<()> {
    if look(space)?.is_dirty() {
        return Err(Box::new(crate::error::DuhError::UncommittedChanges));
    }
    let from = space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;
    let exists = space.list_branches()?.iter().any(|b| b == name);
    let to = match (exists, create) {
        (true, false) => space.resolve_ref_name(ObjectReference::Ref(name.to_string()))?,
        (true, true) => return Err(format!("there's already a draft called '{}'", name).into()),
        (false, true) => {
            space.set_ref(name, ObjectReference::Hash(from), Some("open: new draft"))?;
            from
        }
        (false, false) => {
            return Err(format!("there's no draft called '{}' (`duh open --new {}` starts one)", name, name).into())
        }
    };
    space.set_ref("HEAD", ObjectReference::Ref(name.to_string()), Some("open"))?;
    worktree::match_version(space, from, to)
}

pub fn labels(space: &Space) -> OpResult<Vec<Label>> {
    let mut out = Vec::new();
    for name in space.list_ref_names("tags")? {
        let version = space.resolve_ref_name(ObjectReference::Ref(format!("tags/{}", name)))?;
        out.push(Label { name, version });
    }
    Ok(out)
}

/// Label a version (default: the last save).
pub fn add_label(space: &mut Space, name: &str, version: Option<&ObjectReference>) -> OpResult<Hash> {
    let key = format!("tags/{}", name);
    if space.get_ref(key.clone()).is_ok() {
        return Err(format!("the label '{}' is already used", name).into());
    }
    let commit = resolve_or_head(space, version)?;
    if commit.is_zero() {
        return Err("nothing to label: no saves yet".into());
    }
    space.set_ref(&key, ObjectReference::Hash(commit), Some("label"))?;
    Ok(commit)
}

pub fn remove_label(space: &mut Space, name: &str) -> OpResult<()> {
    let key = format!("tags/{}", name);
    if space.get_ref(key.clone()).is_err() {
        return Err(format!("there's no label '{}'", name).into());
    }
    space.delete_ref(&key)
}

/// What rewinding does to your files and picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rewind {
    /// The undone saves' changes come back picked, ready to save again.
    KeepPicked,
    /// Your files stay as they are and nothing is picked.
    KeepFiles,
    /// Your files are made to match the version; unsaved work is lost.
    Discard,
}

/// Move the open draft (or the version you're looking at) to `to`
/// (default: the last save). Returns the version it's now at.
pub fn rewind(space: &mut Space, to: Option<&ObjectReference>, mode: Rewind) -> OpResult<Hash> {
    let old_head = space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;
    let target = resolve_or_head(space, to)?;
    if !matches!(space.get_object(target)?, Some(Object::Commit(_))) {
        return Err(format!("{} isn't a version", target.to_hex()).into());
    }
    if mode == Rewind::KeepPicked && !space.index.is_empty() {
        // Picked versions are diffs against the old last save; they can't
        // be carried onto a different one.
        return Err("--keep-picked needs nothing picked yet; save or un-pick first".into());
    }

    let msg = format!("rewind to {}", target.to_hex());
    match head_ref(space)? {
        ObjectReference::Ref(draft) => space.set_ref(&draft, ObjectReference::Hash(target), Some(&msg))?,
        _ => space.set_ref("HEAD", ObjectReference::Hash(target), Some(&msg))?,
    }

    match mode {
        Rewind::KeepPicked => pick_differences(space, old_head, target)?,
        Rewind::KeepFiles => space.index.clear(),
        Rewind::Discard => {
            space.index.clear();
            worktree::match_version(space, old_head, target)?;
        }
    }
    space.save_index()?;
    Ok(target)
}

/// After the draft moved from `old_head` to `target`, pick every difference
/// so saving reproduces `old_head`.
fn pick_differences(space: &mut Space, old_head: Hash, target: Hash) -> OpResult<()> {
    if old_head.is_zero() {
        return Ok(());
    }
    let old_files = space.get_commit_files(old_head)?;
    let new_files = space.get_commit_files(target)?;
    let mut paths: Vec<&String> = old_files.iter().filter(|(p, v)| new_files.get(*p) != Some(v)).map(|(p, _)| p).collect();
    paths.sort();

    let mut ops = FileOps::from_space(space);
    for path in paths {
        let content = ops.open_file(path.clone(), old_head)?;
        ops.stage_reader(path.clone(), content, None::<fn(_)>, None::<fn(_)>)?;
    }
    for path in new_files.keys().filter(|p| !old_files.contains_key(*p)) {
        ops.stage_deletion(path.clone())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{pick::pick_file, save::save, testing};
    use super::*;

    #[test]
    fn drafts_labels_and_rewinds() {
        let (dir, mut space) = testing::space();
        testing::write(&dir, "a.bin", b"one");
        pick_file(&mut space, "a.bin", |_| {}).unwrap();
        let v1 = save(&mut space, "one", false).unwrap().version;
        testing::write(&dir, "a.bin", b"two");
        pick_file(&mut space, "a.bin", |_| {}).unwrap();
        let v2 = save(&mut space, "two", false).unwrap().version;

        start_draft(&mut space, "alt", Some(&ObjectReference::Hash(v1))).unwrap();
        open_draft(&mut space, "alt", false).unwrap();
        assert_eq!(std::fs::read(dir.path().join("a.bin")).unwrap(), b"one");
        assert!(delete_draft(&mut space, "alt").unwrap_err().to_string().contains("the draft you have open"));
        rename_draft(&mut space, "alt2").unwrap();
        assert!(drafts(&space).unwrap().iter().any(|d| d.name == "alt2" && d.open && d.version == v1));

        // move_draft resolves what it's given.
        assert_eq!(move_draft(&mut space, &ObjectReference::Ref("main".into())).unwrap(), v2);
        assert_eq!(drafts(&space).unwrap().iter().find(|d| d.name == "alt2").unwrap().version, v2);
        assert!(move_draft(&mut space, &ObjectReference::Ref("nonsense".into())).is_err());

        add_label(&mut space, "first", Some(&ObjectReference::Hash(v1))).unwrap();
        assert!(add_label(&mut space, "first", None).is_err());
        assert_eq!(labels(&space).unwrap()[0].version, v1);

        // Discard makes the files match; KeepPicked picks the difference back.
        assert_eq!(rewind(&mut space, Some(&ObjectReference::Hash(v1)), Rewind::KeepPicked).unwrap(), v1);
        assert_eq!(space.index.len(), 1);
        save(&mut space, "two again", false).unwrap();
        rewind(&mut space, Some(&ObjectReference::Hash(v1)), Rewind::Discard).unwrap();
        assert_eq!(std::fs::read(dir.path().join("a.bin")).unwrap(), b"one");
        remove_label(&mut space, "first").unwrap();
        assert!(labels(&space).unwrap().is_empty());
    }
}
