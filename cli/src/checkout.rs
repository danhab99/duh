use std::error::Error;
use std::fs;
use std::io::Read;

use clap::clap_derive::Args;
use lib::hash::Hash;
use lib::objects::{Object, ObjectReference};
use lib::repo::Repo;

#[derive(Args)]
#[command(about = "Restore a file's contents from a specified commit into the working directory")]
pub struct CheckoutCommand {
    /// Path to the file to checkout
    #[arg(help = "Path to the file to restore (written to the working directory)")]
    pub file_path: String,

    /// Commit hash (64 chars) or the literal `HEAD`. Defaults to `HEAD`.
    #[arg(short = 'c', long = "commit", help = "Commit to read from (use `HEAD` or a full 64-character hash)")]
    pub commit: Option<ObjectReference>,
}

pub fn checkout<F: vfs::FileSystem>(repo: &mut Repo<F>, cmd: &CheckoutCommand) -> Result<(), Box<dyn Error>> {
    // Resolve commit (default to HEAD)
    let target_hash = match &cmd.commit {
        Some(r) => repo.resolve_ref_name(r.clone())?,
        None => repo.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?,
    };

    // Ensure the commit exists and the file is present in that commit
    match repo.get_object(target_hash)? {
        Some(Object::Commit(c)) => {
            let fp = repo.get_path_in_cwd_str(&cmd.file_path);
            if c.files.get(fp.as_str()).is_none() {
                println!("{} {}",
                    crate::colors::red("file not found in commit:"),
                    crate::colors::yellow(&cmd.file_path)
                );
                return Ok(());
            }
        }
        _ => {
            println!("{} {}",
                crate::colors::red("commit not found:"),
                crate::colors::cyan(&target_hash.to_string())
            );
            return Ok(());
        }
    }

    // Reconstruct and write the file to working directory without
    // materializing the whole contents in memory.
    let mut reader = repo.open_file(cmd.file_path.clone(), target_hash)?;

    let out_path = repo.get_path_in_cwd_str(&cmd.file_path);
    let mut out_file = fs::File::create(out_path)?;

    // copy stream directly from the repo reader to the output file
    std::io::copy(&mut reader, &mut out_file)?;

    println!("{} {} @ {}",
        crate::colors::green("checked out"),
        crate::colors::cyan(&cmd.file_path),
        crate::colors::cyan(&target_hash.to_string())
    );
    Ok(())
}
