use std::error::Error;
use std::fs;
use std::path::PathBuf;

use clap::clap_derive::Args;
use lib::file::FileOps;
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
    let to_worktree = cmd.worktree || !cmd.staged;
    if cmd.staged && cmd.source.is_some() {
        return Err("--from with --staged isn't supported yet; unpick, then bring back".into());
    }
    if cmd.as_path.is_some() && cmd.paths.len() != 1 {
        return Err("--as takes exactly one file".into());
    }

    let mut repo_paths = Vec::new();
    for p in &cmd.paths {
        repo_paths.push(space.repo_path_from(&cwd, p)?);
    }

    if cmd.staged {
        crate::unstage::unpick_paths(space, &repo_paths);
    }
    if !to_worktree {
        return Ok(());
    }

    let commit = match &cmd.source {
        Some(r) => space.resolve_ref_name(r.clone())?,
        None => space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?,
    };
    let commit_files = if commit.is_zero() {
        Default::default()
    } else {
        space.get_commit_files(commit)?
    };

    for p in &repo_paths {
        let picked = cmd.source.is_none() && space.index.get(p).is_some_and(|h| !h.is_zero());
        let mut fileops = FileOps::from_space(space);
        let mut reader = if picked {
            fileops.open_staged(p.clone())?
        } else if commit_files.contains_key(p) {
            fileops.open_file(p.clone(), commit)?
        } else if cmd.source.is_some() {
            return Err(format!("'{}' isn't in that version", p).into());
        } else {
            return Err(format!("'{}' isn't a file duh knows", p).into());
        };

        let out = match &cmd.as_path {
            Some(a) => PathBuf::from(a),
            None => space.get_path_in_worktree(p)?,
        };
        if let Some(parent) = out.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        std::io::copy(&mut reader, &mut fs::File::create(&out)?)?;
        match &cmd.as_path {
            Some(a) => println!("Brought back {} as {}", p, a),
            None => println!("Brought back {}", p),
        }
    }
    Ok(())
}
