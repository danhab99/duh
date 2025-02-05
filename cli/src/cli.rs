use clap::{Parser, Subcommand};

use crate::{commit::CommitCommand, diff::DiffCommand, status::StatusCommand, track::TrackCommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Status(StatusCommand),
    Init,
    Diff(DiffCommand),
    Track(TrackCommand),
    Commit(CommitCommand),
}
