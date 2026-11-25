use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::ObjectReference;
use lib::space::Space;

/// Show where a draft has pointed over time
#[derive(Args)]
#[command(about = "Every place a draft has pointed, for recovering lost work")]
pub struct ReflogCommand {
    #[arg(short, long = "draft", alias = "branch", help = "Draft to show (default: the one you're on)")]
    pub branch: Option<String>,
}

pub fn reflog(space: &mut Space, cmd: &ReflogCommand) -> Result<(), Box<dyn Error>> {
    let draft = match cmd.branch.clone() {
        Some(b) => b,
        None => match space.get_ref("HEAD".to_string())? {
            ObjectReference::Ref(r) => r,
            _ => return Err("you're not on a draft; pass --draft".into()),
        },
    };
    for e in lib::ops::history::journal(space, &draft)? {
        println!("{} | {}", e.version.to_string(), e.what);
    }
    Ok(())
}
