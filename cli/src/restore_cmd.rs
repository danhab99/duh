use std::error::Error;
use std::path::Path;

use clap::clap_derive::Args;
use lib::objects::ObjectReference;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Bring back a file as it was: from your picks, the last save, or any version")]
pub struct RestoreCommand {
    /// Files to bring back, relative to the current directory
    #[arg(required = true)]
    pub paths: Vec<String>,

    /// Version to bring back: a label, short hash, HEAD~1, … (default: your
    /// picked version, else the last save)
    #[arg(long = "from", value_name = "VERSION", aliases = ["source", "commit"], short_aliases = ['s', 'c'])]
    pub source: Option<ObjectReference>,

    /// Write the file here instead of over the working copy (one file only)
    #[arg(long = "as", value_name = "PATH")]
    pub as_path: Option<String>,

    /// Un-pick instead: take the files out of the next save (= duh unpick)
    #[arg(short = 'S', long = "staged", hide = true)]
    pub staged: bool,

    /// With --staged, also bring back the working copy
    #[arg(short = 'W', long = "worktree", hide = true)]
    pub worktree: bool,
}

pub fn restore(space: &mut Space, cmd: &RestoreCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    if cmd.staged && cmd.source.is_some() {
        return Err("--from with --staged isn't supported yet; unpick, then bring back".into());
    }
    let mut repo_paths = Vec::new();
    for p in &cmd.paths {
        repo_paths.push(space.repo_path_from(&cwd, p)?);
    }
    if cmd.staged {
        crate::unstage::unpick_paths(space, &repo_paths)?;
        if !cmd.worktree {
            return Ok(());
        }
    }

    // --as is relative to where you are.
    let as_path = cmd.as_path.as_ref().map(|a| cwd.join(a));
    lib::ops::files::bring_back(space, &repo_paths, cmd.source.as_ref(), as_path.as_deref())?;
    for p in &repo_paths {
        match &cmd.as_path {
            Some(a) => println!("Brought back {} as {}", p, Path::new(a).display()),
            None => println!("Brought back {}", p),
        }
    }
    Ok(())
}
