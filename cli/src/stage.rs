use std::error::Error;

use clap::clap_derive::Args;
use lib::repo::Repo;

#[derive(Args)]
pub struct StageCommand {
    /// Path to the file to snapshot
    pub file_path: String,
}

pub fn stage(repo: &mut Repo, cmd: &StageCommand) -> Result<(), Box<dyn Error>> {
    println!("Staging file {}", cmd.file_path);
    let h = repo.stage_file(cmd.file_path.clone())?;

    println!("{}", h.to_string());

    Ok(())
}
