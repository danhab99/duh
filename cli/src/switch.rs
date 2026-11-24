use std::error::Error;

use clap::clap_derive::Args;
use lib::hash::Hash;
use lib::objects::ObjectReference;
use lib::space::Space;

use crate::checkout::checkout_tree;
use crate::status;

#[derive(Args)]
#[command(about = "Switch to another branch, updating the working tree")]
pub struct SwitchCommand {
    #[arg(help = "Name of branch to switch to")]
    pub name: String,

    #[arg(help = "Create the branch at the current commit first")]
    pub create: Option<bool>,
}

pub fn switch(space: &mut Space, cmd: &SwitchCommand) -> Result<(), Box<dyn Error>> {
    if status::collect(space)?.is_dirty() {
        return Err(Box::new(lib::error::DuhError::UncommittedChanges));
    }

    let name = cmd.name.clone();
    let from = space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;
    let exists = space.list_branches()?.contains(&name);

    let to: Hash = match (exists, cmd.create == Some(true)) {
        (true, false) => space.resolve_ref_name(ObjectReference::Ref(name.clone()))?,
        (true, true) => return Err(format!("branch '{}' already exists", name).into()),
        (false, true) => {
            space.set_ref(&name, ObjectReference::Hash(from), Some("switch: create"))?;
            from
        }
        (false, false) => {
            return Err(format!(
                "branch '{}' does not exist (use `duh switch {} true` to create it)",
                name, name
            )
            .into())
        }
    };

    space.set_ref("HEAD", ObjectReference::Ref(name.clone()), Some("switch"))?;
    checkout_tree(space, from, to)?;

    println!("{} {}", crate::colors::cyan("Switched to branch"), name);
    Ok(())
}
