use std::error::Error;

use clap::clap_derive::Args;
use lib::repo::Repo;

/// Show the commit currently referenced by HEAD
#[derive(Args)]
#[command(about = "Push local changes to the remote")]
pub struct PushCommand {
    #[arg(
        short = 'r',
        long = "remote",
        help = "The remote to push to (default: origin)"
    )]
    pub remote_name: Option<String>,
}

pub fn push<F: vfs::FileSystem>(repo: &mut Repo<F>, cmd: &PushCommand) -> Result<(), Box<dyn Error>> {
    let mut remote = repo.get_remote_by_name(cmd.remote_name.as_deref().unwrap_or("origin"))?;

    let h = repo.get_head_commit_hash()?;

    lib::remote::push_branch_to_remote(repo, &mut remote, h, |_| {})?;
    Ok(())
}
