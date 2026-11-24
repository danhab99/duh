use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::ObjectReference;
use lib::space::Space;

/// Show where a draft has pointed over time
#[derive(Args)]
#[command(about = "Every place a draft has pointed, for recovering lost work")]
pub struct ReflogCommand {
    #[arg(
        short,
        long = "draft",
        alias = "branch",
        help = "Draft to show (default: the one you're on)"
    )]
    pub branch: Option<String>,
}

pub fn reflog(space: &mut Space, cmd: &ReflogCommand) -> Result<(), Box<dyn Error>> {
    let branch_name = match cmd.branch.clone() {
        Some(b) => b,
        None => match space.get_ref("HEAD".to_string())? {
            ObjectReference::Ref(r) => r,
            _ => return Err("you're not on a draft; pass --draft".into()),
        },
    };

    let reflog = space.get_reflog(&branch_name)?;
    print!("{}", reflog);
    Ok(())
}
