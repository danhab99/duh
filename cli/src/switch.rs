use std::error::Error;

use clap::clap_derive::Args;
use lib::hash::Hash;
use lib::objects::ObjectReference;
use lib::space::Space;

use crate::checkout::checkout_tree;
use crate::status;

#[derive(Args)]
#[command(about = "Open another draft, swapping your files to match it")]
pub struct SwitchCommand {
    #[arg(help = "Draft to open")]
    pub name: String,

    /// Start a new draft from where you are, then open it
    #[arg(short = 'n', long = "new", alias = "create", short_alias = 'c')]
    pub create: bool,
}

pub fn switch(space: &mut Space, cmd: &SwitchCommand) -> Result<(), Box<dyn Error>> {
    if status::collect(space)?.is_dirty() {
        return Err(Box::new(lib::error::DuhError::UncommittedChanges));
    }

    let name = cmd.name.clone();
    let from = space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;
    let exists = space.list_branches()?.contains(&name);

    let to: Hash = match (exists, cmd.create) {
        (true, false) => space.resolve_ref_name(ObjectReference::Ref(name.clone()))?,
        (true, true) => return Err(format!("there's already a draft called '{}'", name).into()),
        (false, true) => {
            space.set_ref(&name, ObjectReference::Hash(from), Some("open: new draft"))?;
            from
        }
        (false, false) => {
            return Err(format!(
                "there's no draft called '{}' (`duh open --new {}` starts one)",
                name, name
            )
            .into())
        }
    };

    space.set_ref("HEAD", ObjectReference::Ref(name.clone()), Some("open"))?;
    checkout_tree(space, from, to)?;

    println!("{} {}", crate::colors::cyan("Opened draft"), name);
    Ok(())
}
