use clap::*;

use crate::{commit::CommitCommand, snapshot::SnapshotCommand, stage::StageCommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Init,
    Snapshot(SnapshotCommand),
    Stage(StageCommand),
    Commit(CommitCommand),
}
