use std::error::Error;
use std::path::PathBuf;

use clap::clap_derive::Args;
use lib::objects::Object;
use lib::repo::Repo;

#[derive(Args)]
#[command(about = "Stage a file and immediately create a commit (convenience command)")]
pub struct SnapshotCommand {
    /// Path to the file to snapshot
    #[arg(help = "Path to the file to snapshot and commit")]
    pub file_path: String,

    /// Commit message
    #[arg(short = 'm', long = "message", default_value = "Snapshot commit", help = "Commit message for the snapshot")]
    pub message: String,
}

pub fn snapshot(repo: &mut Repo, cmd: &SnapshotCommand) -> Result<(), Box<dyn Error>> {
    println!("Staging file");
    repo.stage_file(cmd.file_path.clone())?;
    println!("Committing file");
    repo.commit(cmd.message.clone())?;
    Ok(())
}
