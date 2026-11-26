use clap::*;

use crate::{
    branch::BranchCommand, check::CheckCommand, commit::CommitCommand, config::ConfigCommand,
    copy::{CloneCommand, ReceiveCommand, SendCommand}, diff::DiffCommand, init::InitCommand,
    log::LogCommand, mv::MvCommand, reflog::ReflogCommand, remote::RemoteCommand,
    reset::ResetCommand, restore_cmd::RestoreCommand, rm::{ForgetCommand, RmCommand},
    show::ShowCommand, stage::StageCommand, status::StatusCommand, switch::SwitchCommand,
    tag::TagCommand, unstage::UnstageCommand,
};

/// Version control for files that aren't text
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Top-level subcommands for the `duh` CLI.
#[derive(Subcommand)]
pub enum Commands {
    // Begin
    /// Start a new space in this folder   [git: init]
    #[command(name = "start", long_about = "Start a space in the current folder. duh makes a hidden .duh folder here to keep every version in; your files stay where they are and nothing leaves this computer until you send it to a place.\n\nWith --bare the space has no working files, only history: use it for a folder other people send to and get copies from. You rarely need this, since the first send to an empty folder or bucket makes one.", alias = "init")]
    Init(InitCommand),

    /// Get a copy of a space into a new folder   [git: clone]
    #[command(name = "get", long_about = "Copy a space into a new folder: every save, with the draft opened and its files written out. The space it came from is added as the place `origin`, so `duh send` and `duh receive` work straight away.\n\nThe URL can be a folder, a file:// URL, any supported storage URL (s3://, gcs://, webdav://, sftp://, ...) or an http(s) address of a published .duh folder. Give storage credentials with -o; they're saved in the new space's settings in plain text.", alias = "clone")]
    Clone(CloneCommand),

    // Your work
    /// See what's changed, picked and new   [git: status]
    #[command(name = "look", long_about = "List the files in this space by state: picked for the next save, changed but not picked, missing, new and not tracked yet, and saved and unchanged.\n\nFiles and folders whose names start with a dot, and files matched by .duhignore, aren't listed. A file can appear as both picked and changed when it was edited again after picking; pick it again to include the newer edits.", alias = "status")]
    Status(StatusCommand),

    /// Compare your files with the last save, or your picks with --picked   [git: diff]
    #[command(name = "compare", long_about = "Show how much of each file changed, as a bar of 60 columns (`=` kept, `-` removed, `+` added) and the totals. Binary files have no lines, so duh compares bytes; a small edit always shows as at least one column.\n\nBy default this compares your files with what's picked, or with the last save. With --picked it compares what's picked with the last save: what the next save would contain.", alias = "diff")]
    Diff(DiffCommand),

    /// Pick files to go into the next save   [git: add]
    #[command(name = "pick", long_about = "Choose what goes into the next save. Picking reads the file as it is now and works out what changed since the last save; edit it again later and you'll need to pick it again.\n\nGive files, folders (`.` for everything here) or patterns. A folder picks changed and new files under it and the deletion of missing ones; -u leaves out new files; -A picks everything in the space. Naming a missing tracked file picks its deletion. Naming a hidden or ignored file picks it anyway.", aliases = ["add", "stage"])]
    Stage(StageCommand),

    /// Un-pick files, keeping your edits   [git: restore --staged]
    #[command(name = "unpick", long_about = "Take files out of the next save. Your edits to them stay in your files, untouched.", alias = "unstage")]
    Unstage(UnstageCommand),

    /// Save a new version of your picked files   [git: commit]
    #[command(name = "save", long_about = "Save a new version of what's picked, on the draft you have open. Every tracked file is in the save: the picked ones as you picked them, the rest as they were in the save before. Only the bytes that changed are stored.\n\nWith -a, every changed or deleted tracked file is picked first. Naming one file picks and saves just that file. Without -m, duh opens your EDITOR for the message, or writes one from what changed when EDITOR isn't set (or with -g).", alias = "commit")]
    Commit(CommitCommand),

    /// Bring back a file as it was in your picks, the last save, or any version   [git: restore, checkout]
    #[command(name = "bring-back", long_about = "Bring back files as they were. With no --from: the picked version if the file is picked, otherwise the last save; this throws away unsaved edits to it. With --from, the file as it was in any version.\n\nWith --as, the file is written to that path instead, leaving yours untouched: the safe way to look at an old version. Bringing a file back never changes history; save afterwards if you want that version to be the newest.", aliases = ["restore", "checkout"])]
    Restore(RestoreCommand),

    /// Rename or move a tracked file   [git: mv]
    #[command(name = "rename", long_about = "Rename or move a tracked file, on disk and in the next save. The new path is stored as changes to the old one, so renaming costs almost nothing. The destination can be an existing folder.", aliases = ["mv", "move"])]
    Mv(MvCommand),

    /// Delete files and stop tracking them   [git: rm]
    #[command(name = "delete", long_about = "Delete files from the folder and stop tracking them in the next save. Earlier versions stay in history and can be brought back. Refuses files with unsaved changes unless --force; --cached keeps the files on disk (the same as forget).", alias = "rm")]
    Rm(RmCommand),

    /// Stop tracking files but keep them on disk   [git: rm --cached]
    #[command(name = "forget", long_about = "Stop tracking files in the next save, but leave them on disk. Earlier versions stay in history. Add them to .duhignore if you don't want look to offer them again.")]
    Forget(ForgetCommand),

    // Versions
    /// Show your saves, newest first   [git: log]
    #[command(name = "history", long_about = "List the saves on the open draft, newest first: hash, who saved it and when, and the message. Labels and drafts pointing at a save are shown beside it. Give a version to start from there instead, -n to show fewer, and --oneline for a short hash and the message only.", alias = "log")]
    Log(LogCommand),

    /// Inspect one version: who saved it, when, and its files   [git: show]
    #[command(name = "inspect", long_about = "Show one save in detail: its hash, the save before it, who made it and when, its message, and every file in it. Default: the last save.", alias = "show")]
    Show(ShowCommand),

    /// Every place a draft has pointed, for recovering lost work   [git: reflog]
    #[command(name = "journal", long_about = "List every point the open draft has been at, oldest first, with why it moved: a save, a rewind, a receive, being created. Saves that are no longer on any draft can be found here and recovered by starting a draft at them (`duh drafts recovered <hash>`).\n\nThe journal of a deleted draft is kept: `duh journal -b <name>` still lists it. The journal records saves only, never unsaved edits.", alias = "reflog")]
    Reflog(ReflogCommand),

    /// Name a version, e.g. "final-v2"   [git: tag]
    #[command(name = "label", long_about = "Give a save a name, like final-v2 or sent-to-printer, that can be used anywhere a version is asked for. Labels don't move. With no name, list labels; -d removes one (the save stays). Labels stay in this space: sending doesn't copy them.", alias = "tag")]
    Tag(TagCommand),

    /// Move your draft back (or forward) to another version   [git: reset]
    #[command(name = "rewind", long_about = "Move the open draft to another version, as if the later saves hadn't happened on it. The saves rewound past aren't deleted; they stay in the journal.\n\nBy default your files are left as they are and nothing is picked. --keep-picked also picks the undone changes, ready to save again (for fixing the last save's message, or adding a forgotten file). --discard makes your files match the version, throwing away unsaved work.", alias = "reset")]
    Reset(ResetCommand),

    // Drafts
    /// List, create, rename or delete drafts   [git: branch]
    #[command(name = "drafts", long_about = "A draft is a line of saves; every space starts with `main`. With no arguments, list drafts, with `*` on the open one. With a name, start a draft at a version (default: where you are) without opening it.\n\n-m renames the open draft, -d deletes another (its journal stays, so its saves can be found again), and -s points the open draft at another version without touching your files. duh never merges drafts.", aliases = ["draft", "branch"])]
    Branch(BranchCommand),

    /// Open another draft, swapping your files to match it   [git: switch]
    #[command(name = "open", long_about = "Open another draft, changing your files to match its latest save. Refuses while you have unsaved changes, since they'd be overwritten: save them on the current draft, or bring back your files first. With --new, start the draft where you are and open it.", alias = "switch")]
    Switch(SwitchCommand),

    // Sharing
    /// Send your saves to a place   [git: push]
    #[command(name = "send", long_about = "Send your saves to a place (default: origin), updating the draft of the same name there, or the one given with --draft. Only what the place lacks is copied. The first send to an empty folder or bucket creates the space there.\n\nSending only ever moves the place's draft forward. If the place has saves you don't, it refuses: receive first, or send to a new draft name. --force overwrites the place's draft anyway, discarding the saves only it has.", alias = "push")]
    Send(SendCommand),

    /// Receive new saves from a place   [git: pull]
    #[command(name = "receive", long_about = "Receive new saves from a place (default: origin) into your draft of the same name, or the one given with --into; --draft picks the draft on the place. Receiving into the open draft updates your files, so it refuses while you have unsaved changes.\n\nReceiving only ever moves your draft forward. duh doesn't merge: if you and the place have both saved, it refuses. Receive into a new draft instead (`--draft main --into theirs`) to keep both.", alias = "pull")]
    Receive(ReceiveCommand),

    /// Manage places your space is shared to (folders, S3, Drive, …)   [git: remote]
    #[command(name = "places", long_about = "A place is somewhere else a copy of the space lives: a folder, a USB drive, a NAS, or cloud storage (S3, GCS, Azure, WebDAV, SFTP, Google Drive, Dropbox and more). duh writes to it directly; there's no server. Options and credentials for a place are stored in .duh/config in plain text.", aliases = ["place", "remote"])]
    Remote(RemoteCommand),

    /// View or change settings   [git: config]
    #[command(name = "settings", long_about = "Read or change this space's settings, stored in .duh/config: user.name and user.email (on new saves), chunk_size, max_size and solid_after (how files are stored), and remote.<place>.url and remote.<place>.config.<option> (places). Values that would stop the space from opening are refused.", alias = "config")]
    Config(ConfigCommand),

    /// Check that every save is intact and reads back exactly   [git: fsck]
    #[command(name = "check", long_about = "Prove every save is intact: read every stored piece and re-hash it, follow every link between them, and rebuild every version of every file, comparing each with the hash recorded when it was saved. Picked files and solid copies are checked too.\n\nGive a place to check that copy instead, such as a backup. Exits with status 1 and lists each problem if anything is wrong.", alias = "fsck")]
    Check(CheckCommand),
}
