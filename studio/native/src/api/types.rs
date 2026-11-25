//! Data duh Studio gets from the engine. Hashes are hex strings; sizes and
//! times are `i64` (times in seconds since 1970, UTC).

/// A space that's open.
#[derive(Clone, Debug)]
pub struct SpaceInfo {
    /// The folder with the space's files.
    pub root: String,
    /// The folder's name, for showing.
    pub name: String,
    /// The open draft, if any.
    pub draft: Option<String>,
}

/// What kind of bytes a run in a change strip is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    Kept,
    Added,
    Removed,
}

/// One run of bytes in a change strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub kind: SegmentKind,
    pub len: i64,
}

/// Where a file stands in the folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkState {
    /// Picked for the next save, and not changed since.
    Picked,
    /// Changed since the last save (or since it was picked).
    Changed,
    /// Tracked, but gone from the folder.
    Missing,
    /// In the folder, not tracked yet.
    New,
    /// Saved, unchanged.
    Unchanged,
}

/// What a pick will do to a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickedKind {
    None,
    New,
    Modified,
    Deleted,
}

/// One file in the folder.
#[derive(Clone, Debug)]
pub struct WorkFile {
    pub path: String,
    pub state: WorkState,
    /// What's picked for it (`None` if nothing).
    pub picked: PickedKind,
    /// Size on disk (0 if missing).
    pub size: i64,
}

/// The folder at a glance.
#[derive(Clone, Debug)]
pub struct LookResult {
    pub draft: Option<String>,
    /// The last save ("" before the first).
    pub head: String,
    pub files: Vec<WorkFile>,
}

/// Progress while picking.
#[derive(Clone, Debug)]
pub struct Progress {
    pub path: String,
    /// Files done so far, of `total`.
    pub done: u32,
    pub total: u32,
}

/// What picking one file did.
#[derive(Clone, Debug)]
pub struct Picked {
    pub path: String,
    /// Whether anything was picked (false: identical to the last save).
    pub changed: bool,
    pub deleted: bool,
    pub strip: Vec<Segment>,
}

/// A save just made.
#[derive(Clone, Debug)]
pub struct Saved {
    pub version: String,
    pub draft: Option<String>,
}

/// One save, as the timeline shows it.
#[derive(Clone, Debug)]
pub struct SaveInfo {
    pub version: String,
    pub previous: String,
    pub message: String,
    pub author: String,
    pub time: i64,
    pub labels: Vec<String>,
    pub drafts: Vec<String>,
    /// Bytes added and removed across its files.
    pub added: i64,
    pub removed: i64,
    /// How many files it changed.
    pub files_changed: u32,
}

/// A file in a save.
#[derive(Clone, Debug)]
pub struct FileInSave {
    pub path: String,
    pub size: i64,
    /// Whether this save changed it.
    pub changed: bool,
    pub strip: Vec<Segment>,
}

/// One save in detail.
#[derive(Clone, Debug)]
pub struct SaveDetail {
    pub info: SaveInfo,
    pub files: Vec<FileInSave>,
}

/// One journal entry: where a draft pointed, and why.
#[derive(Clone, Debug)]
pub struct JournalItem {
    pub version: String,
    pub what: String,
}

#[derive(Clone, Debug)]
pub struct DraftInfo {
    pub name: String,
    pub version: String,
    pub open: bool,
}

#[derive(Clone, Debug)]
pub struct LabelInfo {
    pub name: String,
    pub version: String,
}

/// What rewinding does to your files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewindMode {
    /// The undone saves' changes come back picked.
    KeepPicked,
    /// Your files stay as they are; nothing is picked.
    KeepFiles,
    /// Your files are made to match; unsaved work is lost.
    Discard,
}

#[derive(Clone, Debug)]
pub struct PlaceInfo {
    pub name: String,
    pub url: String,
}

/// How the open draft compares with the same draft on a place.
#[derive(Clone, Debug)]
pub struct SyncInfo {
    pub to_send: u32,
    pub to_receive: u32,
    pub place_has_draft: bool,
    pub diverged: bool,
}

/// What a send or receive did.
#[derive(Clone, Debug)]
pub struct TransferResult {
    pub draft: String,
    pub saves: u32,
    pub bytes: i64,
}

/// The result of checking a space.
#[derive(Clone, Debug)]
pub struct CheckResult {
    pub objects: u32,
    pub saves: u32,
    pub file_versions: u32,
    pub solids: u32,
    pub problems: Vec<String>,
}
