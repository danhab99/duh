use std::error::Error;

use clap::clap_derive::Args;
use lib::{hash::Hash, objects::ObjectReference, space::Space};

use crate::checkout::checkout;

#[derive(Args)]
#[command(about = "Copy commits between local and remote repositories")]
pub struct CopyCommand {
    #[arg(
        short = 'f',
        long = "from",
        help = "Remote name to copy from (pulls commits into a new local branch)"
    )]
    pub from: Option<String>,

    #[arg(
        short = 't',
        long = "to",
        help = "Remote name to copy to (pushes commits to a new remote branch)"
    )]
    pub to: Option<String>,

    #[arg(
        short = 'a',
        long = "as",
        required = true,
        help = "Name for the new branch that will receive the copied commits"
    )]
    pub as_branch: String,

    #[arg(
        short = 'b',
        long = "branch",
        help = "Remote branch to copy (defaults to local HEAD branch name, or remote HEAD if not found)"
    )]
    pub branch: Option<String>,
}

pub fn copy(
    space: &mut Space,
    cmd: &CopyCommand,
) -> Result<(), Box<dyn Error>> {
    match (&cmd.from, &cmd.to) {
        (Some(remote_name), None) => {
            copy_from(space, remote_name.as_str(), &cmd.as_branch, cmd.branch.as_deref())?
        }
        (None, Some(remote_name)) => {
            copy_to(space, remote_name.as_str(), &cmd.as_branch, cmd.branch.as_deref())?
        }
        (Some(_), Some(_)) => {
            return Err("cannot specify both --from and --to".into());
        }
        (None, None) => {
            return Err("must specify either --from or --to".into());
        }
    }

    Ok(())
}

fn copy_from(
    space: &mut Space,
    remote_name: &str,
    branch_name: &str,
    remote_branch: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let mut remote = space.get_remote_by_name(remote_name)?;

    let target_branch = remote_branch.unwrap_or_else(|| {
        space.get_head_branch_name().unwrap_or_else(|_| "HEAD".to_string())
    });

    let remote_hash = remote
        .resolve_ref_name(ObjectReference::Ref(target_branch.clone()))
        .unwrap_or_else(|_| remote.get_head_commit_hash().unwrap_or_else(|_| Hash::new()));

    lib::remote::copy_commits::<_, _, fn(lib::remote::CopyCommitsProgress)>(
        &mut remote,
        space,
        remote_hash,
        None,
    )?;

    space.set_ref(
        branch_name,
        ObjectReference::Hash(remote_hash),
        Some(format!("copy from {}: {}", remote_name, branch_name).as_str()),
    )?;

    space.set_ref(
        "HEAD",
        ObjectReference::Ref(branch_name.to_string()),
        None,
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
        crate::colors::green(branch_name)
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

    let target_branch = remote_branch.unwrap_or_else(|| {
        space.get_head_branch_name().unwrap_or_else(|_| branch_name.to_string())
    });

    remote.set_ref(
        &target_branch,
        ObjectReference::Hash(head_hash),
        Some(format!("copy to: {}", target_branch).as_str()),
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
