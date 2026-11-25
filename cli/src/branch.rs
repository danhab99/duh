use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::ObjectReference;
use lib::ops::refs;
use lib::space::Space;

#[derive(Args)]
#[command(about = "List, create, rename or delete drafts")]
pub struct BranchCommand {
    /// Start a new draft with this name (lists drafts when omitted)
    name: Option<String>,

    /// Version to start it from (default: where you are)
    start: Option<String>,

    #[arg(short = 'd', long = "delete", help = "Delete a draft")]
    delete: Option<String>,

    #[arg(short = 'm', long = "move", visible_alias = "rename", help = "Rename the draft you're on")]
    rename: Option<String>,

    #[arg(short = 's', long = "set", help = "Point the draft you're on at another version")]
    set: Option<String>,
}

pub fn branch(space: &mut Space, cmd: &BranchCommand) -> Result<(), Box<dyn Error>> {
    if let Some(name) = &cmd.name {
        let start = cmd.start.clone().map(ObjectReference::from);
        let at = refs::start_draft(space, name, start.as_ref())?;
        println!("Started draft {} at {} (`duh open {}` to switch to it)", name, &at.to_string()[..12], name);
    } else if let Some(name) = &cmd.delete {
        refs::delete_draft(space, name)?;
        println!("Deleted draft {}", name);
    } else if let Some(name) = &cmd.rename {
        refs::rename_draft(space, name)?;
        println!("Renamed the draft to {}", name);
    } else if let Some(set) = &cmd.set {
        let at = refs::move_draft(space, &ObjectReference::from(set.clone()))?;
        println!("Moved the draft to {}", &at.to_string()[..12]);
    } else {
        for d in refs::drafts(space)? {
            println!("{} {}", if d.open { "*" } else { " " }, d.name);
        }
    }
    Ok(())
}
