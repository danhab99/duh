use std::error::Error;
use std::path::PathBuf;

use clap::clap_derive::Args;
use lib::{hash::Hash, objects::ObjectReference, space::Space};
use opendal::services::Fs;

use crate::checkout::checkout;

#[derive(Args)]
#[command(about = "Copy commits between local and remote repositories, or clone a remote")]
pub struct CopyCommand {
    #[arg(
        short = 'f',
        long = "from",
        help = "Remote name to copy from (defaults to 'origin'), or a URL to clone from when outside a duh space"
    )]
    pub from: Option<String>,

    #[arg(
        short = 't',
        long = "to",
        help = "Remote name to copy to (pushes commits to a remote branch)"
    )]
    pub to: Option<String>,

    #[arg(
        short = 'a',
        long = "as",
        help = "Local branch name to receive/send commits (defaults to current HEAD branch), or destination directory name when cloning"
    )]
    pub as_branch: Option<String>,

    #[arg(
        short = 'b',
        long = "branch",
        help = "Remote branch to copy (defaults to local HEAD branch name, or remote HEAD if not found)"
    )]
    pub branch: Option<String>,
}

/// Clone a remote repository into a new directory (git clone equivalent)
pub fn clone(
    url: &str,
    dest_dir: &str,
    remote_branch: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    use std::fs;

    let dest_path = PathBuf::from(dest_dir);

    if dest_path.exists() {
        return Err(format!("destination '{}' already exists", dest_dir).into());
    }

    // Create the destination directory
    fs::create_dir_all(&dest_path)?;

    // Initialize a new duh space
    let op = Fs::default().root(dest_path.to_str().unwrap());
    let afs = opendal::Operator::new(op)?.finish();
    let fs = opendal::blocking::Operator::new(afs)?;
    let mut space = Space::initialize_at(fs, Some(dest_path.clone()))?;

    // Add the remote as "origin"
    space.add_remote("origin", url)?;

    // Open the remote
    let mut remote = space.get_remote_by_name("origin")?;

    // Determine which branch to clone
    let head_name = remote
        .get_head_branch_name()
        .unwrap_or_else(|_| "HEAD".to_string());
    let target_branch = remote_branch.unwrap_or(&head_name);

    // Resolve the remote branch hash
    let remote_hash = remote
        .resolve_ref_name(ObjectReference::Ref(target_branch.to_string()))
        .unwrap_or_else(|_| remote.get_head_commit_hash().unwrap_or_else(|_| Hash::new()));

    // Copy commits from remote
    lib::remote::copy_commits(
        &mut remote,
        &mut space,
        remote_hash,
        None::<fn(lib::remote::CopyCommitsProgress)>,
    )?;

    // Set up local branch tracking the remote
    let local_branch = target_branch;
    space.set_ref(
        local_branch,
        ObjectReference::Hash(remote_hash),
        Some(&format!("clone from {}: {}", "origin", local_branch)),
    )?;

    space.set_ref(
        "HEAD",
        ObjectReference::Ref(local_branch.to_string()),
        Some("clone from remote"),
    )?;

    // Checkout all files
    let files = space.list_files(ObjectReference::Hash(remote_hash))?;

    for file_path in files {
        checkout(
            &mut space,
            &crate::checkout::CheckoutCommand {
                file_path,
                commit: Some(ObjectReference::Hash(remote_hash)),
                as_name: None,
            },
        )?;
    }

    space.save_index()?;

    println!(
        "{} cloned {} -> {}",
        crate::colors::cyan("Cloned"),
        crate::colors::green(url),
        crate::colors::green(dest_dir)
    );

    Ok(())
}

pub fn copy(
    space: &mut Space,
    cmd: &CopyCommand,
) -> Result<(), Box<dyn Error>> {
    let head_branch = space.get_head_branch_name().unwrap_or_else(|_| "HEAD".to_string());
    let branch_name = cmd.as_branch.as_deref().unwrap_or(&head_branch);

    match (&cmd.from, &cmd.to) {
        (Some(remote_name), None) => {
            copy_from(space, remote_name.as_str(), branch_name, cmd.branch.as_deref())?
        }
        (None, Some(remote_name)) => {
            copy_to(space, remote_name.as_str(), branch_name, cmd.branch.as_deref())?
        }
        (Some(_), Some(_)) => {
            return Err("cannot specify both --from and --to".into());
        }
        (None, None) => {
            // Default to copying from "origin"
            copy_from(space, "origin", branch_name, cmd.branch.as_deref())?
        }
    }

    Ok(())
}

fn copy_from(
    space: &mut Space,
    remote_name: &str,
    local_branch: &str,
    remote_branch: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let mut remote = space.get_remote_by_name(remote_name)?;

    let target_remote_branch = remote_branch.unwrap_or(local_branch);

    let remote_hash = remote
        .resolve_ref_name(ObjectReference::Ref(target_remote_branch.to_string()))
        .unwrap_or_else(|_| remote.get_head_commit_hash().unwrap_or_else(|_| Hash::new()));

    lib::remote::copy_commits(
        &mut remote,
        space,
        remote_hash,
        None::<fn(lib::remote::CopyCommitsProgress)>,
    )?;

    space.set_ref(
        local_branch,
        ObjectReference::Hash(remote_hash),
        Some(&format!("copy from {}: {}", remote_name, local_branch)),
    )?;

    space.set_ref(
        "HEAD",
        ObjectReference::Ref(local_branch.to_string()),
        Some("copy from remote"),
    )?;

    let files = space.list_files(ObjectReference::Hash(remote_hash))?;

    for file_path in files {
        checkout(
            space,
            &crate::checkout::CheckoutCommand {
                file_path,
                commit: Some(ObjectReference::Hash(remote_hash)),
                as_name: None,
            },
        )?;
    }

    println!(
        "{} {} from {} -> {}",
        crate::colors::cyan("Copied"),
        remote_hash.to_hex(),
        remote_name,
        crate::colors::green(local_branch)
    );

    Ok(())
}

fn copy_to(
    space: &mut Space,
    remote_name: &str,
    branch_name: &str,
    remote_branch: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let mut remote = space.get_remote_by_name(remote_name)?;

    let head_hash = space.get_head_commit_hash()?;

    lib::remote::copy_commits(space, &mut remote, head_hash, Some(|_| {}))?;

    let head_name = space.get_head_branch_name().unwrap_or_else(|_| branch_name.to_string());
    let target_branch = remote_branch.unwrap_or(&head_name);

    remote.set_ref(
        &target_branch,
        ObjectReference::Hash(head_hash),
        Some(&format!("copy to: {}", target_branch)),
    )?;

    println!(
        "{} {} to {} -> {}",
        crate::colors::cyan("Copied"),
        head_hash.to_hex(),
        remote_name,
        crate::colors::green(&target_branch)
    );

    Ok(())
}
