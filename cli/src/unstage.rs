use std::error::Error;

use clap::clap_derive::Args;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Un-pick files: leave them out of the next save, keeping your edits")]
pub struct UnstageCommand {
    /// Files to un-pick, relative to the current directory
    #[arg(required = true)]
    pub paths: Vec<String>,
}

/// Un-pick these repo paths, saying what happened.
pub fn unpick_paths(space: &mut Space, repo_paths: &[String]) -> Result<(), Box<dyn Error>> {
    for (p, was_picked) in lib::ops::pick::unpick(space, repo_paths)? {
        if was_picked {
            println!("Un-picked {}", p);
        } else {
            println!("{}", crate::colors::dim(&format!("{} wasn't picked", p)));
        }
    }
    Ok(())
}

pub fn unstage(space: &mut Space, cmd: &UnstageCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let mut repo_paths = Vec::new();
    for p in &cmd.paths {
        repo_paths.push(space.repo_path_from(&cwd, p)?);
    }
    unpick_paths(space, &repo_paths)
}
