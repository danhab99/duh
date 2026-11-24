use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::ObjectReference;
use lib::space::Space;

/// Show the commit currently referenced by HEAD
#[derive(Args)]
#[command(about = "Show the history of updates to a branch")]
pub struct ReflogCommand {
    #[arg(
        short,
        long = "branch",
        help = "Branch whose reflog to read (defaults to the current branch)"
    )]
    pub branch: Option<String>,
}

pub fn reflog(space: &mut Space, cmd: &ReflogCommand) -> Result<(), Box<dyn Error>> {
    let branch_name = match cmd.branch.clone() {
        Some(b) => b,
        None => match space.get_ref("HEAD".to_string())? {
            ObjectReference::Ref(r) => r,
            _ => return Err("HEAD is detached; pass --branch".into()),
        },
    };

    let reflog = space.get_reflog(&branch_name)?;
    print!("{}", reflog);
    Ok(())
}
