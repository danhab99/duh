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
    pub commit: Option<String>,
}

pub fn checkout(repo: &mut Repo, cmd: &CheckoutCommand) -> Result<(), Box<dyn Error>> {
    // Resolve commit (default to HEAD)
    let target_hash = match &cmd.commit {
        Some(s) if s == "HEAD" => repo.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?,
        Some(s) if s.len() == 64 => Hash::from_str(s).clone(),
        Some(_) => {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "commit must be 'HEAD' or a 64-character hash",
            )))
        }
        None => repo.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?,
    };

    // Ensure the commit exists and the file is present in that commit
    match repo.get_object(target_hash)? {
        Some(Object::Commit(c)) => {
            let fp = repo.get_path_in_cwd_str(&cmd.file_path);
            if c.files.get(fp.as_str()).is_none() {
                println!("file '{}' not found in commit {}", cmd.file_path, target_hash.to_string());
                return Ok(());
            }
        }
        _ => {
            println!("commit {} not found", target_hash.to_string());
            return Ok(());
        }
    }

    // Reconstruct and write the file to working directory
    let mut reader = repo.open_file(cmd.file_path.clone(), target_hash)?;
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;

    let out_path = repo.get_path_in_cwd_str(&cmd.file_path);
    fs::write(out_path, buf)?;

    println!("checked out {} @ {}", cmd.file_path, target_hash.to_string());
    Ok(())
}
