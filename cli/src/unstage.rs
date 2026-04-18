use std::error::Error;

use clap::clap_derive::Args;
use lib::repo::Repo;

#[derive(Args)]
#[command(about = "Stage a file (produce fragment objects without committing)")]
pub struct UnstageCommand {
    /// Path to the file to stage
    #[arg(help = "Path to the file to stage (relative to current working directory)")]
    pub file_path: String,
}

pub fn unstage(repo: &mut Repo, cmd: &UnstageCommand) -> Result<(), Box<dyn Error>> {
    println!("{} {}", crate::colors::cyan("Unstaging file"), cmd.file_path);
    repo.unstage_file(cmd.file_path.clone())?;

    println!("Unstaged {}", crate::colors::green(&cmd.file_path.to_string()));

    Ok(())
}
