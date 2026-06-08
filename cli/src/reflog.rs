use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::{Object, ObjectReference};
use lib::space::Space;

/// Show the commit currently referenced by HEAD
#[derive(Args)]
#[command(about = "")]
pub struct ReflogCommand {
    #[arg(short = "b", long = "branch", help = "the branche's reflog to read")]
    pub branch: Option<String>,
}

pub fn reflog(space: &mut Space, cmd: &ReflogCommand) -> Result<(), Box<dyn Error>> {
    let branch_name = cmd.branch.ok_or_else(|| {
        let head = space.get_ref("HEAD".to_string())?;

        match head {
            ObjectReference::Ref(r) => r,
            _ => panic!("on a detached head"),
        }
    })?;

    let reflog = space.get_reflog(branch_name)?;
    print!(reflog);
}
