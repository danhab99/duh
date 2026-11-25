//! Everything duh Studio can ask of duh's engine. Each function takes the
//! folder of a space (any folder inside it), does one thing through
//! `lib::ops`, and returns plain data. Functions that change a space take its
//! writer lock, so the CLI and the app never write at once.
//!
//! Types here are shaped for Dart: hashes are hex strings, sizes and times
//! are `i64`, and enums carry no data.

use flutter_rust_bridge::frb;

use crate::core;

use crate::api::types::*;


/// The version of duh's engine this app was built with.
#[frb(sync)]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}

// Spaces

/// The space containing `folder`, or an error if there isn't one.
pub fn open_space(folder: String) -> anyhow::Result<SpaceInfo> {
    core::open_space(&folder)
}

/// Start a new space in `folder`.
pub fn start_space(folder: String) -> anyhow::Result<SpaceInfo> {
    core::start_space(&folder)
}

/// Start a bare space (history only, for sharing) in an empty `folder`.
pub fn start_bare_space(folder: String) -> anyhow::Result<()> {
    core::start_bare_space(&folder)
}

/// Copy the space at `url` into the new folder `dest`. `options` are
/// storage settings such as credentials, as `key=value`.
pub fn get_space(url: String, dest: String, options: Vec<String>) -> anyhow::Result<SpaceInfo> {
    core::get_space(&url, &dest, &options)
}

// Your files

pub fn look(folder: String) -> anyhow::Result<LookResult> {
    core::look(&folder)
}

/// How your copy of `path` differs from what's picked (or the last save).
pub fn compare_file(folder: String, path: String) -> anyhow::Result<Vec<Segment>> {
    core::compare_file(&folder, &path)
}

/// Pick files (deletions for missing ones).
pub fn pick(folder: String, paths: Vec<String>) -> anyhow::Result<Vec<Picked>> {
    core::pick(&folder, &paths, |_| {})
}

pub fn unpick(folder: String, paths: Vec<String>) -> anyhow::Result<()> {
    core::unpick(&folder, &paths)
}

pub fn suggest_message(folder: String) -> anyhow::Result<String> {
    core::suggest_message(&folder)
}

pub fn save(folder: String, message: String) -> anyhow::Result<Saved> {
    core::save(&folder, &message)
}

/// Bring back a file: from `from` (a version), or else what's picked or the
/// last save. With `as_path`, writes a copy there instead.
pub fn bring_back(folder: String, path: String, from: Option<String>, as_path: Option<String>) -> anyhow::Result<()> {
    core::bring_back(&folder, &path, from.as_deref(), as_path.as_deref())
}

// History

pub fn history(folder: String, max: Option<u32>) -> anyhow::Result<Vec<SaveInfo>> {
    core::history(&folder, max)
}

pub fn inspect(folder: String, version: String) -> anyhow::Result<SaveDetail> {
    core::inspect(&folder, &version)
}

pub fn journal(folder: String, draft: String) -> anyhow::Result<Vec<JournalItem>> {
    core::journal(&folder, &draft)
}

// Drafts and labels

pub fn drafts(folder: String) -> anyhow::Result<Vec<DraftInfo>> {
    core::drafts(&folder)
}

/// Start a draft at `at` (a version; default: the last save).
pub fn start_draft(folder: String, name: String, at: Option<String>) -> anyhow::Result<()> {
    core::start_draft(&folder, &name, at.as_deref())
}

/// Open a draft, making your files match it.
pub fn open_draft(folder: String, name: String) -> anyhow::Result<()> {
    core::open_draft(&folder, &name)
}

pub fn delete_draft(folder: String, name: String) -> anyhow::Result<()> {
    core::delete_draft(&folder, &name)
}

pub fn rename_draft(folder: String, new_name: String) -> anyhow::Result<()> {
    core::rename_draft(&folder, &new_name)
}

pub fn labels(folder: String) -> anyhow::Result<Vec<LabelInfo>> {
    core::labels(&folder)
}

pub fn add_label(folder: String, name: String, version: String) -> anyhow::Result<()> {
    core::add_label(&folder, &name, &version)
}

pub fn remove_label(folder: String, name: String) -> anyhow::Result<()> {
    core::remove_label(&folder, &name)
}

pub fn rewind(folder: String, version: String, mode: RewindMode) -> anyhow::Result<()> {
    core::rewind(&folder, &version, mode)
}

// Places

pub fn places(folder: String) -> anyhow::Result<Vec<PlaceInfo>> {
    core::places(&folder)
}

pub fn add_place(folder: String, name: String, url: String, options: Vec<String>) -> anyhow::Result<()> {
    core::add_place(&folder, &name, &url, &options)
}

pub fn remove_place(folder: String, name: String) -> anyhow::Result<()> {
    core::remove_place(&folder, &name)
}

pub fn sync_status(folder: String, place: String) -> anyhow::Result<SyncInfo> {
    core::sync_status(&folder, &place)
}

/// Send the open draft to `place`, as `draft` there (default: same name).
pub fn send(folder: String, place: String, draft: Option<String>, force: bool) -> anyhow::Result<TransferResult> {
    core::send(&folder, &place, draft.as_deref(), force)
}

/// Receive `draft` from `place` (default: the open draft's name) into your
/// draft `into` (default: the open one).
pub fn receive(folder: String, place: String, draft: Option<String>, into: Option<String>) -> anyhow::Result<TransferResult> {
    core::receive(&folder, &place, draft.as_deref(), into.as_deref())
}

// Health

/// Check that every save is intact, here or (with `place`) in a place.
pub fn check(folder: String, place: Option<String>) -> anyhow::Result<CheckResult> {
    core::check(&folder, place.as_deref())
}
