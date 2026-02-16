use std::error::Error;
use std::path::PathBuf;

use clap::clap_derive::Args;
use lib::objects::Object;
use lib::repo::Repo;

#[derive(Args)]
#[command(about = "Stage the given file and create a commit with the provided message")]
pub struct CommitCommand {
    /// Optional path to the file to snapshot and include in the commit. If
    /// omitted the command will commit the current index (all staged files).
    #[arg(help = "Optional path to a file to stage and commit")]
    pub file_path: Option<String>,

    /// Commit message
    #[arg(short = 'm', long = "message", default_value = "Snapshot commit", help = "Message to store with the new commit")]
    pub message: String,
}

pub fn commit(repo: &mut Repo, cmd: &CommitCommand) -> Result<(), Box<dyn Error>> {
    if let Some(fp) = &cmd.file_path {
        println!("Staging file {}", fp);
        repo.stage_file(fp.clone())?;
    } else {
        println!("No file provided — committing staged files in index");
    }

    println!("Committing");
    repo.commit(cmd.message.clone())?;
    Ok(())
}
