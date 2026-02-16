use clap::*;

use crate::{commit::CommitCommand, snapshot::SnapshotCommand, stage::StageCommand, show::ShowCommand, checkout::CheckoutCommand, log::LogCommand};

/// `duh` — tiny duplicate-aware snapshot tool.
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
    /// Initialize a new repository (create `.duh/` metadata)
    Init,

    /// Snapshot a file and immediately commit it
    Snapshot(SnapshotCommand),

    /// Stage a file (prepare fragments for later commit)
    Stage(StageCommand),

    /// Stage and commit a file (shortcut)
    Commit(CommitCommand),

    /// Show the commit currently pointed to by HEAD
    Show(ShowCommand),

    /// Restore a file's contents from a commit into the working tree
    Checkout(CheckoutCommand),

    /// Show the commit log (history) starting at HEAD
    Log(LogCommand),
}
