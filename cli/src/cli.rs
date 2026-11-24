use clap::*;

use crate::{
    branch::BranchCommand, checkout::CheckoutCommand, commit::CommitCommand, config::ConfigCommand,
    copy::CopyCommand, init::InitCommand, log::LogCommand, reflog::ReflogCommand, remote::RemoteCommand, reset::ResetCommand, show::ShowCommand,
    stage::StageCommand, status::StatusCommand, switch::SwitchCommand, tag::TagCommand,
    unstage::UnstageCommand,
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
    /// Initialize a new space (create `.duh/` metadata)
    Init(InitCommand),

    /// Add file contents to the index (alias: stage)
    #[command(name = "add", visible_alias = "stage")]
    Stage(StageCommand),

    /// Unstage a file (remove the file from being comitted)
    Unstage(UnstageCommand),

    /// Unstage and commit a file (shortcut)
    Commit(CommitCommand),

    /// Show the commit currently pointed to by HEAD
    Show(ShowCommand),

    /// Restore a file's contents from a commit into the working tree
    Checkout(CheckoutCommand),

    /// Show the commit log (history) starting at HEAD
    Log(LogCommand),

    /// Show the history of updates to a branch
    Reflog(ReflogCommand),

    /// Show which tracked files have changed compared to the index
    Status(StatusCommand),

    /// Switch or create a different branch
    Switch(SwitchCommand),

    /// Create delete and manage branches
    Branch(BranchCommand),

    /// Get or set space configuration values
    Config(ConfigCommand),

    /// Copy commits between local and remote repositories
    Copy(CopyCommand),

    /// Manage remote repository connections
    Remote(RemoteCommand),

    /// Create, list, or delete lightweight tags
    Tag(TagCommand),

    /// Reset the current branch to a specified commit
    Reset(ResetCommand),
}
