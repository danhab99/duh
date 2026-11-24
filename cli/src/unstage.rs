use std::error::Error;

use clap::clap_derive::Args;
use lib::file::FileOps;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Unstage a file (remove it from the index)")]
pub struct UnstageCommand {
    /// Path to the file to unstage
    #[arg(help = "Path to the file to unstage (relative to current working directory)")]
    pub file_path: String,
}

pub fn unstage(space: &mut Space, cmd: &UnstageCommand) -> Result<(), Box<dyn Error>> {
    println!("{} {}", crate::colors::cyan("Unstaging file"), cmd.file_path);
    let repo_path = space.repo_path_from(&std::env::current_dir()?, &cmd.file_path)?;
    let mut fileops = FileOps::from_space(space);
    fileops.unstage_file(repo_path)?;

    println!("Unstaged {}", crate::colors::green(&cmd.file_path.to_string()));

    Ok(())
}
