use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::ObjectReference;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Create, list, or delete lightweight tags")]
pub struct TagCommand {
    /// Tag to create (lists tags when omitted)
    name: Option<String>,

    /// Commit to tag (default: HEAD)
    commit: Option<String>,

    #[arg(short = 'd', long = "delete", help = "Delete a tag")]
    delete: Option<String>,

    #[arg(short = 'l', long = "list", default_value_t = false, help = "List all tags")]
    list: bool,
}

pub fn tag(space: &mut Space, cmd: &TagCommand) -> Result<(), Box<dyn Error>> {
    if let Some(name) = cmd.delete.clone() {
        let tag_ref = format!("tags/{}", name);
        if space.get_ref(tag_ref.clone()).is_err() {
            return Err(format!("tag '{}' does not exist", name).into());
        }
        space.delete_ref(tag_ref.as_str())?;
        println!("Deleted tag {}", name);
    } else if let (Some(name), false) = (&cmd.name, cmd.list) {
        let tag_ref = format!("tags/{}", name);
        if space.get_ref(tag_ref.clone()).is_ok() {
            return Err(format!("tag '{}' already exists", name).into());
        }
        let target = ObjectReference::from(cmd.commit.clone().unwrap_or_else(|| "HEAD".into()));
        let commit_hash = space.resolve_ref_name(target)?;
        if commit_hash.is_zero() {
            return Err("nothing to tag: no commits yet".into());
        }
        space.set_ref(tag_ref.as_str(), ObjectReference::Hash(commit_hash), Some("tag"))?;
        println!("Created tag {} -> {}", name, commit_hash.to_string());
    } else {
        for name in space.list_ref_names("tags")? {
            let commit_hash = space.resolve_ref_name(ObjectReference::Ref(format!("tags/{}", name)))?;
            println!("{} -> {}", name, commit_hash.to_string());
        }
    }

    Ok(())
}
