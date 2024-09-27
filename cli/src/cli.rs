use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

pub struct StatusCommand {
    pub wd: Option<String>,
}

pub struct DiffCommand {
    pub old: String,
    pub new: String,
}

pub struct TrackCommand {
    pub names: Vec<String>,
}

pub struct CommitCommand {
    pub start: String,
    pub message: String,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Find out what has changed in this repo
    Status(StatusCommand),
    Init,
    Diff(DiffCommand),
    Track(TrackCommand),
    Commit(CommitCommand),
}
