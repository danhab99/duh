use std::error::Error;
use std::fs;

use clap::clap_derive::Args;
use lib::file::FileOps;
use lib::objects::{Object, ObjectReference};
use lib::space::Space;

#[derive(Args)]
#[command(about = "Restore a file's contents from a specified commit into the working directory")]
pub struct CheckoutCommand {
    /// Path to the file to checkout
    #[arg(help = "Path to the file to restore (written to the working directory)")]
    pub file_path: String,

    /// Commit hash (64 chars) or the literal `HEAD`. Defaults to `HEAD`.
    #[arg(
        short = 'c',
        long = "commit",
        help = "Commit to read from (use `HEAD` or a full 64-character hash)"
    )]
    pub commit: Option<ObjectReference>,

    #[arg(long = "as", help = "checkout a file at a commit as a different name")]
    pub as_name: Option<String>,
}

pub fn checkout(
    space: &mut Space,
    cmd: &CheckoutCommand,
) -> Result<(), Box<dyn Error>> {
    // Resolve commit (default to HEAD)
    let target_hash = match &cmd.commit {
        Some(r) => space.resolve_ref_name(r.clone())?,
        None => space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?,
    };

    // Ensure the commit exists and the file is present in that commit
    match space.get_object(target_hash)? {
        Some(Object::Commit(_c)) => {
            let fp = space.get_path_in_cwd_str(&cmd.file_path);
            let files = space.get_commit_files(target_hash)?;
            if files.get(fp.as_str()).is_none() {
                println!(
                    "{} {}",
                    crate::colors::red("file not found in commit:"),
                    crate::colors::yellow(&cmd.file_path)
                );
                return Ok(());
            }
        }
        _ => {
            println!(
                "{} {}",
                crate::colors::red("commit not found:"),
                crate::colors::cyan(&target_hash.to_string())
            );
            return Ok(());
        }
    }

    // Reconstruct and write the file to working directory without
    // materializing the whole contents in memory.
    let mut fileops = FileOps::from_space(space);
    let mut reader = fileops.open_file(cmd.file_path.clone(), target_hash)?;

    let out_path = cmd
        .as_name
        .clone()
        .or_else(|| Some(space.get_path_in_cwd_str(&cmd.file_path)));
    let mut out_file = fs::File::create(out_path.unwrap())?;

    // copy stream directly from the space reader to the output file
    std::io::copy(&mut reader, &mut out_file)?;

    println!(
        "{} {} @ {}",
        crate::colors::green("checked out"),
        crate::colors::cyan(&cmd.file_path),
        crate::colors::cyan(&target_hash.to_string())
    );
    Ok(())
}
