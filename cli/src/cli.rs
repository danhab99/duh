use clap::*;

use crate::{
    branch::BranchCommand, checkout::CheckoutCommand, commit::CommitCommand, config::ConfigCommand,
    copy::{CloneCommand, CopyCommand}, init::InitCommand, log::LogCommand, reflog::ReflogCommand, rm::RmCommand, restore_cmd::RestoreCommand, diff::DiffCommand, mv::MvCommand, remote::RemoteCommand, reset::ResetCommand, show::ShowCommand,
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
    // Start a space
    /// Create an empty space (a `.duh/` directory) here
    Init(InitCommand),

    /// Clone a repository into a new directory
    Clone(CloneCommand),

    // Work on the current change
    /// Add file contents to the index
    #[command(name = "add", visible_alias = "stage")]
    Stage(StageCommand),

    /// Move or rename a tracked file
    Mv(MvCommand),

    /// Restore working tree files, or unstage them with --staged
    Restore(RestoreCommand),

    /// Remove files from the working tree and the index
    Rm(RmCommand),

    /// Remove files from the index, keeping the working copy (= restore --staged)
    Unstage(UnstageCommand),

    // Examine history and state
    /// Show changes: working tree vs index, or index vs HEAD with --staged
    Diff(DiffCommand),

    /// Show the commit history
    Log(LogCommand),

    /// Show a commit (default: HEAD) and the files it contains
    Show(ShowCommand),

    /// Show the working tree status
    Status(StatusCommand),

    /// Show the history of updates to a branch
    Reflog(ReflogCommand),

    // Grow and tweak history
    /// Record the staged changes as a new commit
    Commit(CommitCommand),

    /// List, create, rename or delete branches
    Branch(BranchCommand),

    /// Switch to another branch, updating the working tree
    Switch(SwitchCommand),

    /// Write a file as of some commit into the working tree (or --as elsewhere)
    Checkout(CheckoutCommand),

    /// Move the current branch to another commit (--soft, --mixed, --hard)
    Reset(ResetCommand),

    /// List, create or delete tags
    Tag(TagCommand),

    // Collaborate
    /// Copy commits to or from a remote (pull/push)
    Copy(CopyCommand),

    /// Manage remote repository connections
    Remote(RemoteCommand),

    /// Get or set space configuration values
    Config(ConfigCommand),
}
