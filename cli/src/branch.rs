use std::error::Error;

use clap::clap_derive::Args;
use lib::{objects::ObjectReference, space::Space};

#[derive(Args)]
#[command(about = "List, delete, rename and move branches")]
pub struct BranchCommand {
    /// Create a branch with this name (lists branches when omitted)
    name: Option<String>,

    /// Commit to start the new branch at (default: HEAD)
    start: Option<String>,

    #[arg(short = 'd', long = "delete", help = "Delete a branch")]
    delete: Option<String>,

    #[arg(short = 'm', long = "move", visible_alias = "rename", help = "Rename the current branch")]
    rename: Option<String>,

    #[arg(
        short = 's',
        long = "set",
        help = "Set the commit this branch points to"
    )]
    set: Option<String>,
}

pub fn branch(space: &mut Space, cmd: &BranchCommand) -> Result<(), Box<dyn Error>> {
    let head_ref = space.get_ref("HEAD".into())?;

    if let Some(name) = cmd.name.clone() {
        if space.list_branches()?.contains(&name) {
            return Err(format!("branch '{}' already exists", name).into());
        }
        let start = ObjectReference::from(cmd.start.clone().unwrap_or_else(|| "HEAD".into()));
        let commit = space.resolve_ref_name(start)?;
        if commit.is_zero() {
            return Err("can't create a branch before the first commit".into());
        }
        space.set_ref(&name, ObjectReference::Hash(commit), Some("branch: created"))?;
        println!("Created branch {} at {}", name, &commit.to_string()[..12]);
    } else if let Some(name) = cmd.delete.clone() {
        if head_ref == ObjectReference::Ref(name.clone()) {
            return Err(format!("cannot delete branch '{}': it is checked out", name).into());
        }
        if !space.list_branches()?.contains(&name) {
            return Err(format!("branch '{}' does not exist", name).into());
        }
        space.delete_ref(name.as_str())?;
        println!("Deleted branch {}", name);
    } else if let Some(name) = cmd.rename.clone() {
        let current_commit = space.resolve_ref_name(head_ref.clone())?;

        space.delete_ref(head_ref.to_string().as_str())?;
        space.set_ref(&name, ObjectReference::Hash(current_commit), Some("rename branch"))?;
        // Update HEAD to point to the new branch name; the old ref was just deleted.
        space.set_ref("HEAD", ObjectReference::Ref(name.clone()), Some("rename branch"))?;
    } else if let Some(set) = cmd.set.clone() {
        let commit = ObjectReference::from(set);
        space.set_ref(head_ref.to_string().as_str(), commit, Some("set branch"))?;
    } else {
        for name in space.list_branches()? {
            let marker = if head_ref == ObjectReference::Ref(name.clone()) { "*" } else { " " };
            println!("{} {}", marker, name);
        }
    }

    Ok(())
}
