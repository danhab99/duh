use clap::*;

use crate::{
    branch::BranchCommand, check::CheckCommand, commit::CommitCommand, config::ConfigCommand,
    copy::{CloneCommand, ReceiveCommand, SendCommand}, diff::DiffCommand, init::InitCommand,
    log::LogCommand, mv::MvCommand, reflog::ReflogCommand, remote::RemoteCommand,
    reset::ResetCommand, restore_cmd::RestoreCommand, rm::{ForgetCommand, RmCommand},
    show::ShowCommand, stage::StageCommand, status::StatusCommand, switch::SwitchCommand,
    tag::TagCommand, unstage::UnstageCommand,
};

/// `duh` — binary oriented version control
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
    #[command(name = "start", alias = "init")]
    Init(InitCommand),

    /// Get a copy of a space into a new folder   [git: clone]
    #[command(name = "get", alias = "clone")]
    Clone(CloneCommand),

    // Your work
    /// See what's changed, picked and new   [git: status]
    #[command(name = "look", alias = "status")]
    Status(StatusCommand),

    /// Compare your files with the last save, or your picks with --picked   [git: diff]
    #[command(name = "compare", alias = "diff")]
    Diff(DiffCommand),

    /// Pick files to go into the next save   [git: add]
    #[command(name = "pick", aliases = ["add", "stage"])]
    Stage(StageCommand),

    /// Un-pick files, keeping your edits   [git: restore --staged]
    #[command(name = "unpick", alias = "unstage")]
    Unstage(UnstageCommand),

    /// Save a new version of your picked files   [git: commit]
    #[command(name = "save", alias = "commit")]
    Commit(CommitCommand),

    /// Bring back a file as it was in your picks, the last save, or any version   [git: restore, checkout]
    #[command(name = "bring-back", aliases = ["restore", "checkout"])]
    Restore(RestoreCommand),

    /// Rename or move a tracked file   [git: mv]
    #[command(name = "rename", aliases = ["mv", "move"])]
    Mv(MvCommand),

    /// Delete files and stop tracking them   [git: rm]
    #[command(name = "delete", alias = "rm")]
    Rm(RmCommand),

    /// Stop tracking files but keep them on disk   [git: rm --cached]
    #[command(name = "forget")]
    Forget(ForgetCommand),

    // Versions
    /// Show your saves, newest first   [git: log]
    #[command(name = "history", alias = "log")]
    Log(LogCommand),

    /// Inspect one version: who saved it, when, and its files   [git: show]
    #[command(name = "inspect", alias = "show")]
    Show(ShowCommand),

    /// Every place a draft has pointed, for recovering lost work   [git: reflog]
    #[command(name = "journal", alias = "reflog")]
    Reflog(ReflogCommand),

    /// Name a version, e.g. "final-v2"   [git: tag]
    #[command(name = "label", alias = "tag")]
    Tag(TagCommand),

    /// Move your draft back (or forward) to another version   [git: reset]
    #[command(name = "rewind", alias = "reset")]
    Reset(ResetCommand),

    // Drafts
    /// List, create, rename or delete drafts   [git: branch]
    #[command(name = "drafts", aliases = ["draft", "branch"])]
    Branch(BranchCommand),

    /// Open another draft, swapping your files to match it   [git: switch]
    #[command(name = "open", alias = "switch")]
    Switch(SwitchCommand),

    // Sharing
    /// Send your saves to a place   [git: push]
    #[command(name = "send", alias = "push")]
    Send(SendCommand),

    /// Receive new saves from a place   [git: pull]
    #[command(name = "receive", alias = "pull")]
    Receive(ReceiveCommand),

    /// Manage places your space is shared to (folders, S3, Drive, …)   [git: remote]
    #[command(name = "places", aliases = ["place", "remote"])]
    Remote(RemoteCommand),

    /// View or change settings   [git: config]
    #[command(name = "settings", alias = "config")]
    Config(ConfigCommand),

    /// Check that every save is intact and reads back exactly   [git: fsck]
    #[command(name = "check", alias = "fsck")]
    Check(CheckCommand),
}
