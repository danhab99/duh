use std::error::Error;
use std::path::PathBuf;

use clap::clap_derive::Args;
use lib::objects::Object;
use lib::repo::Repo;

#[derive(Args)]
#[command(about = "Stage the given file and create a commit with the provided message")]
pub struct CommitCommand {
    /// Path to the file to snapshot and commit
    #[arg(help = "Path to the file to snapshot and include in the commit")]
    pub file_path: String,

    /// Commit message
    #[arg(short = 'm', long = "message", default_value = "Snapshot commit", help = "Message to store with the new commit")]
    pub message: String,
}

pub fn commit(repo: &mut Repo, cmd: &CommitCommand) -> Result<(), Box<dyn Error>> {
    println!("Staging file");
    repo.stage_file(cmd.file_path.clone())?;
    println!("Committing file");
    repo.commit(cmd.message.clone())?;
    Ok(())
}
