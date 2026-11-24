use std::error::Error;

use clap::clap_derive::Args;
use lib::{objects::ObjectReference, space::Space};

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

    #[arg(
        short = 's',
        long = "set",
        help = "Point the draft you're on at another version"
    )]
    set: Option<String>,
}

pub fn branch(space: &mut Space, cmd: &BranchCommand) -> Result<(), Box<dyn Error>> {
    let head_ref = space.get_ref("HEAD".into())?;

    if let Some(name) = cmd.name.clone() {
        if space.list_branches()?.contains(&name) {
            return Err(format!("there's already a draft called '{}'", name).into());
        }
        let start = ObjectReference::from(cmd.start.clone().unwrap_or_else(|| "HEAD".into()));
        let commit = space.resolve_ref_name(start)?;
        if commit.is_zero() {
            return Err("save something first; a draft starts from a version".into());
        }
        space.set_ref(&name, ObjectReference::Hash(commit), Some("draft: created"))?;
        println!("Started draft {} at {} (`duh open {}` to switch to it)", name, &commit.to_string()[..12], name);
    } else if let Some(name) = cmd.delete.clone() {
        if head_ref == ObjectReference::Ref(name.clone()) {
            return Err(format!("can't delete '{}': it's the draft you have open", name).into());
        }
        if !space.list_branches()?.contains(&name) {
            return Err(format!("there's no draft called '{}'", name).into());
        }
        space.delete_ref(name.as_str())?;
        println!("Deleted draft {}", name);
    } else if let Some(name) = cmd.rename.clone() {
        let current_commit = space.resolve_ref_name(head_ref.clone())?;

        space.delete_ref(head_ref.to_string().as_str())?;
        space.set_ref(&name, ObjectReference::Hash(current_commit), Some("draft: renamed"))?;
        // Update HEAD to point to the new branch name; the old ref was just deleted.
        space.set_ref("HEAD", ObjectReference::Ref(name.clone()), Some("draft: renamed"))?;
    } else if let Some(set) = cmd.set.clone() {
        let commit = ObjectReference::from(set);
        space.set_ref(head_ref.to_string().as_str(), commit, Some("draft: moved"))?;
    } else {
        for name in space.list_branches()? {
            let marker = if head_ref == ObjectReference::Ref(name.clone()) { "*" } else { " " };
            println!("{} {}", marker, name);
        }
    }

    Ok(())
}
