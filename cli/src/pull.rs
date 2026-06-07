use std::error::Error;

use clap::clap_derive::Args;
use lib::space::Space;

/// Show the commit currently referenced by HEAD
#[derive(Args)]
#[command(about = "Pull changes from the remote")]
pub struct PullCommand {
    #[arg(
        short = 'r',
        long = "remote",
        help = "The remote to pull from (default: origin)"
    )]
    pub remote_name: Option<String>,
}

pub fn pull<F: vfs::FileSystem>(
    space: &mut Space<F>,
    cmd: &PullCommand,
) -> Result<(), Box<dyn Error>> {
    let remote_name = cmd.remote_name.as_deref().unwrap_or("origin");
    let mut remote = space.get_remote_by_name(remote_name)?;

    lib::remote::copy_commits(space, &mut remote, remote_name)?;
    Ok(())
}
