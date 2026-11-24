use std::error::Error;
use std::fs;
use std::path::Path;

use clap::clap_derive::Args;
use lib::file::FileOps;
use lib::space::Space;

use crate::status;

#[derive(Args)]
#[command(about = "Move or rename a tracked file")]
pub struct MvCommand {
    /// Tracked file to move
    pub source: String,

    /// New path (or an existing directory to move it into)
    pub destination: String,
}

pub fn mv(space: &mut Space, cmd: &MvCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let report = status::collect(space)?;

    let src = space.repo_path_from(&cwd, &cmd.source)?;
    if report.tracked().binary_search(&src).is_err() {
        return Err(format!("'{}' is not tracked", cmd.source).into());
    }
    let src_full = space.get_path_in_worktree(&src)?;
    if !src_full.is_file() {
        return Err(format!("'{}' is missing from your files", cmd.source).into());
    }

    // Moving into an existing directory keeps the file name.
    let mut dest_arg = Path::new(&cmd.destination).to_path_buf();
    if dest_arg.is_dir() {
        dest_arg.push(src_full.file_name().unwrap());
    }
    let dest = space.repo_path_from(&cwd, &dest_arg.to_string_lossy())?;
    let dest_full = space.get_path_in_worktree(&dest)?;
    if dest_full.exists() {
        return Err(format!("destination '{}' already exists", dest).into());
    }

    if let Some(parent) = dest_full.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&src_full, &dest_full)?;

    // Diff the new path against the old path's committed content, so the
    // move costs nothing in storage. A file that was only staged has no
    // committed content to reuse.
    let head = space.get_head_commit_hash()?;
    let base = if !head.is_zero() && space.get_commit_files(head)?.contains_key(&src) {
        Some(src.clone())
    } else {
        None
    };
    let mut fileops = FileOps::from_space(space);
    fileops.stage_deletion(src.clone())?;
    fileops.stage_reader_from(
        dest.clone(),
        base,
        Box::new(fs::File::open(&dest_full)?),
        None::<fn(_)>,
        None::<fn(_)>,
    )?;

    println!("Renamed {} -> {}", src, dest);
    Ok(())
}
