use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::ObjectReference;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Name a version (like \"final-v2\"), list names, or remove one")]
pub struct TagCommand {
    /// Label to create (lists labels when omitted)
    name: Option<String>,

    /// Version to label (default: the last save)
    #[arg(value_name = "VERSION")]
    commit: Option<String>,

    #[arg(short = 'd', long = "delete", help = "Remove a label (the version stays)")]
    delete: Option<String>,

    #[arg(short = 'l', long = "list", default_value_t = false, help = "List all labels")]
    list: bool,
}

pub fn tag(space: &mut Space, cmd: &TagCommand) -> Result<(), Box<dyn Error>> {
    if let Some(name) = cmd.delete.clone() {
        let tag_ref = format!("tags/{}", name);
        if space.get_ref(tag_ref.clone()).is_err() {
            return Err(format!("there's no label '{}'", name).into());
        }
        space.delete_ref(tag_ref.as_str())?;
        println!("Removed label {}", name);
    } else if let (Some(name), false) = (&cmd.name, cmd.list) {
        let tag_ref = format!("tags/{}", name);
        if space.get_ref(tag_ref.clone()).is_ok() {
            return Err(format!("the label '{}' is already used", name).into());
        }
        let target = ObjectReference::from(cmd.commit.clone().unwrap_or_else(|| "HEAD".into()));
        let commit_hash = space.resolve_ref_name(target)?;
        if commit_hash.is_zero() {
            return Err("nothing to label: no saves yet".into());
        }
        space.set_ref(tag_ref.as_str(), ObjectReference::Hash(commit_hash), Some("label"))?;
        println!("Labelled {} as {}", &commit_hash.to_string()[..12], name);
    } else {
        for name in space.list_ref_names("tags")? {
            let commit_hash = space.resolve_ref_name(ObjectReference::Ref(format!("tags/{}", name)))?;
            println!("{} -> {}", name, commit_hash.to_string());
        }
    }

    Ok(())
}
