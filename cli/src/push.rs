use std::error::Error;

use clap::clap_derive::Args;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Push commits to a remote repository")]
pub struct PushCommand {
    /// Remote name to push to (defaults to 'origin')
    #[arg(short = 'r', long = "remote", help = "Remote name to push to")]
    pub remote: Option<String>,

    /// Branch name to push to (defaults to current HEAD branch)
    #[arg(short = 'b', long = "branch", help = "Remote branch to push to")]
    pub branch: Option<String>,
}

pub fn push(space: &mut Space, cmd: &PushCommand) -> Result<(), Box<dyn Error>> {
    let remote_name = cmd.remote.as_deref().unwrap_or("origin");
    let head_hash = space.get_head_commit_hash()?;
    let head_branch = space.get_head_branch_name().unwrap_or_else(|_| "HEAD".to_string());
    let target_branch = cmd.branch.as_deref().unwrap_or(&head_branch);

    let mut remote = space.get_remote_by_name(remote_name)?;

    lib::remote::copy_commits(space, &mut remote, head_hash, Some(|_| {}))?;

    remote.set_ref(
        target_branch,
        lib::objects::ObjectReference::Hash(head_hash.clone()),
        Some(&format!("push: {}", target_branch)),
    )?;

    println!(
        "{} {} to {} -> {}",
        crate::colors::cyan("Pushed"),
        crate::colors::yellow(&head_hash.to_hex()),
        crate::colors::yellow(remote_name),
        crate::colors::green(target_branch)
    );

    Ok(())
}
