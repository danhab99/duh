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

/// Drop these repo paths from the picked set, saying what happened.
pub fn unpick_paths(space: &mut Space, repo_paths: &[String]) {
    for p in repo_paths {
        if space.index.remove(p).is_some() {
            println!("Un-picked {}", p);
        } else {
            println!("{}", crate::colors::dim(&format!("{} wasn't picked", p)));
        }
    }
}

pub fn unstage(space: &mut Space, cmd: &UnstageCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let mut repo_paths = Vec::new();
    for p in &cmd.paths {
        repo_paths.push(space.repo_path_from(&cwd, p)?);
    }
    unpick_paths(space, &repo_paths);
    Ok(())
}
