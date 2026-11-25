//! Looking back: `duh history`, `duh inspect` and `duh journal`.

use std::collections::HashMap;

use super::{OpResult, Span};
use crate::file::FileOps;
use crate::hash::Hash;
use crate::objects::{Object, ObjectReference};
use crate::space::Space;

/// One save, as the timeline shows it.
#[derive(Clone, Debug)]
pub struct SaveInfo {
    pub version: Hash,
    /// The save before it (zero for the first).
    pub previous: Hash,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
    /// Seconds since 1970 (UTC).
    pub time: u64,
    /// Labels on this save.
    pub labels: Vec<String>,
    /// Drafts whose last save this is.
    pub drafts: Vec<String>,
}

/// A file as it is in a save.
#[derive(Clone, Debug)]
pub struct FileInSave {
    pub path: String,
    /// The file version object.
    pub version: Hash,
    pub size: u64,
    /// How it differs from the version it was saved against (all added for
    /// a file new in this save; empty when the save carried it unchanged).
    pub strip: Vec<Span>,
    /// Whether this save changed it (false: carried over unchanged).
    pub changed: bool,
}

/// One save in detail.
#[derive(Clone, Debug)]
pub struct SaveDetail {
    pub info: SaveInfo,
    /// Every file in the save, sorted by path.
    pub files: Vec<FileInSave>,
}

/// One entry of a draft's journal: where it pointed and why.
#[derive(Clone, Debug)]
pub struct JournalEntry {
    pub version: Hash,
    pub what: String,
}

/// The version a reference names (a label, draft, short hash, `HEAD~2`…).
pub fn resolve(space: &Space, r: &ObjectReference) -> OpResult<Hash> {
    space.resolve_ref_name(r.clone())
}

/// Labels and drafts by the save they point at.
fn names_by_save(space: &Space) -> OpResult<(HashMap<Hash, Vec<String>>, HashMap<Hash, Vec<String>>)> {
    let mut labels: HashMap<Hash, Vec<String>> = HashMap::new();
    for name in space.list_ref_names("tags")? {
        if let Ok(h) = space.resolve_ref_name(ObjectReference::Ref(format!("tags/{}", name))) {
            labels.entry(h).or_default().push(name);
        }
    }
    let mut drafts: HashMap<Hash, Vec<String>> = HashMap::new();
    for name in space.list_branches()? {
        if let Ok(h) = space.resolve_ref_name(ObjectReference::Ref(name.clone())) {
            drafts.entry(h).or_default().push(name);
        }
    }
    Ok((labels, drafts))
}

fn info(space: &Space, version: Hash, names: &(HashMap<Hash, Vec<String>>, HashMap<Hash, Vec<String>>)) -> OpResult<SaveInfo> {
    match space.get_object(version)? {
        Some(Object::Commit(c)) => Ok(SaveInfo {
            version,
            previous: c.parent,
            message: c.message,
            author_name: c.author.name,
            author_email: c.author.email,
            time: c.author.timestamp,
            labels: names.0.get(&version).cloned().unwrap_or_default(),
            drafts: names.1.get(&version).cloned().unwrap_or_default(),
        }),
        _ => Err(format!("{} isn't a version", version.to_hex()).into()),
    }
}

/// Saves from `from` back to the first, newest first, at most `max`.
pub fn history(space: &Space, from: Hash, max: Option<usize>) -> OpResult<Vec<SaveInfo>> {
    let names = names_by_save(space)?;
    let mut out = Vec::new();
    let mut cur = from;
    while !cur.is_zero() && max.is_none_or(|m| out.len() < m) {
        let save = info(space, cur, &names)?;
        cur = save.previous;
        out.push(save);
    }
    Ok(out)
}

/// One save in detail: who, when, and every file with its size and how
/// this save changed it.
pub fn inspect(space: &mut Space, version: Hash) -> OpResult<SaveDetail> {
    let names = names_by_save(space)?;
    let info = info(space, version, &names)?;
    let before = if info.previous.is_zero() { Default::default() } else { space.get_commit_files(info.previous)? };
    let mut files = Vec::new();
    let mut entries: Vec<(String, Hash)> = space.get_commit_files(version)?.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, file_version) in entries {
        let changed = before.get(&path) != Some(&file_version);
        let strip: Vec<Span> = match space.get_object(file_version)? {
            Some(Object::File(f)) if changed => FileOps::from_space(space).load_fragments(&f)?.iter().map(Span::from).collect(),
            _ => Vec::new(),
        };
        let size = if changed {
            strip.iter().filter(|s| !matches!(s, Span::Removed(_))).map(|s| s.len()).sum()
        } else {
            size_of(space, &path, version)?
        };
        files.push(FileInSave { path, version: file_version, size, strip, changed });
    }
    Ok(SaveDetail { info, files })
}

/// A file's size in a save, without reading it.
fn size_of(space: &mut Space, path: &str, version: Hash) -> OpResult<u64> {
    use std::io::Seek;
    let mut r = FileOps::from_space(space).open_file(path.to_string(), version)?;
    Ok(r.seek(std::io::SeekFrom::End(0))?)
}

/// Everywhere `draft` has pointed, oldest first.
pub fn journal(space: &Space, draft: &str) -> OpResult<Vec<JournalEntry>> {
    let text = space.get_reflog(draft)?;
    let mut out = Vec::new();
    for line in text.lines() {
        let (hash, what) = line.split_once('|').unwrap_or((line, ""));
        if let Ok(version) = Hash::from_string(hash.trim().to_string()) {
            out.push(JournalEntry { version, what: what.trim().to_string() });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::{pick::pick_file, refs, save::save, testing};
    use super::*;

    #[test]
    fn history_inspect_and_journal() {
        let (dir, mut space) = testing::space();
        let mut data = testing::noise(1, 40_000);
        testing::write(&dir, "a.bin", &data);
        testing::write(&dir, "b.bin", b"steady");
        pick_file(&mut space, "a.bin", |_| {}).unwrap();
        pick_file(&mut space, "b.bin", |_| {}).unwrap();
        let first = save(&mut space, "first", false).unwrap().version;
        data.extend_from_slice(&testing::noise(2, 5_000));
        testing::write(&dir, "a.bin", &data);
        pick_file(&mut space, "a.bin", |_| {}).unwrap();
        let second = save(&mut space, "second", false).unwrap().version;
        refs::add_label(&mut space, "v1", Some(&ObjectReference::Hash(first))).unwrap();

        let saves = history(&space, second, None).unwrap();
        assert_eq!(saves.iter().map(|s| s.message.as_str()).collect::<Vec<_>>(), ["second", "first"]);
        assert_eq!(saves[0].drafts, ["main"]);
        assert_eq!(saves[1].labels, ["v1"]);
        assert_eq!(history(&space, second, Some(1)).unwrap().len(), 1);

        let detail = inspect(&mut space, second).unwrap();
        let a = &detail.files[0];
        assert!(a.changed && a.size == data.len() as u64);
        // The appended bytes, plus the chunk they joined at the old end.
        assert!(super::super::strip_totals(&a.strip).0 >= 5_000);
        let b = &detail.files[1];
        assert!(!b.changed && b.strip.is_empty() && b.size == 6);

        let j = journal(&space, "main").unwrap();
        assert_eq!(j.last().unwrap().version, second);
    }
}
