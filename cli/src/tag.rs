use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::ObjectReference;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Create, list, or delete lightweight tags")]
pub struct TagCommand {
    #[arg(short = 'd', long = "delete", help = "Delete a tag")]
    delete: Option<String>,

    #[arg(short = 'l', long = "list", default_value_t = false, help = "List all tags")]
    list: bool,

    #[arg(short = 'n', long = "name", help = "Tag name (for create)")]
    name: Option<String>,

    #[arg(short = 'r', long = "ref", help = "Commit ref or hash to tag (for create)")]
    commit: Option<String>,
}

pub fn tag(space: &mut Space, cmd: &TagCommand) -> Result<(), Box<dyn Error>> {
    if let Some(name) = cmd.delete.clone() {
        let tag_ref = format!("tags/{}", name);
        if space.get_ref(tag_ref.clone()).is_err() {
            return Err(format!("tag '{}' does not exist", name).into());
        }
        println!("Deleting tag {}", name);
        space.delete_ref(tag_ref.as_str())?;
    } else if let (Some(name), Some(commit_ref)) = (&cmd.name, &cmd.commit) {
        let commit = ObjectReference::from(commit_ref.to_string());
        let commit_hash = space.resolve_ref_name(commit)?;
        let tag_ref = format!("tags/{}", name);
        space.set_ref(tag_ref.as_str(), ObjectReference::Hash(commit_hash), Some("tag"))?;
        println!("Created tag {} -> {}", name, commit_hash.to_string());
    } else {
        println!("Listing all tags");
        let refs = space.list_refs("tags")?;

        for r in refs {
            if let ObjectReference::Ref(tag_name) = r {
                let commit_hash = space.resolve_ref_name(ObjectReference::Ref(format!("tags/{}", tag_name)))?;
                println!("  {} -> {}", tag_name, commit_hash.to_string());
            }
        }
    }

    Ok(())
}
