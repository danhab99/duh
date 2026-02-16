use std::error::Error;

use clap::clap_derive::Args;
use lib::repo::Repo;

#[derive(Args)]
#[command(about = "Stage a file (produce fragment objects without committing)")]
pub struct StageCommand {
    /// Path to the file to stage
    #[arg(help = "Path to the file to stage (relative to current working directory)")]
    pub file_path: String,
}

pub fn stage(repo: &mut Repo, cmd: &StageCommand) -> Result<(), Box<dyn Error>> {
    println!("{} {}", crate::colors::cyan("Staging file"), cmd.file_path);
    let h = repo.stage_file(cmd.file_path.clone())?;

    println!("{}", crate::colors::green(&h.to_string()));

    Ok(())
}
