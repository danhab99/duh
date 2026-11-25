use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::ObjectReference;
use lib::ops::refs;
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
    if let Some(name) = &cmd.delete {
        refs::remove_label(space, name)?;
        println!("Removed label {}", name);
    } else if let (Some(name), false) = (&cmd.name, cmd.list) {
        let version = cmd.commit.clone().map(ObjectReference::from);
        let at = refs::add_label(space, name, version.as_ref())?;
        println!("Labelled {} as {}", &at.to_string()[..12], name);
    } else {
        for l in refs::labels(space)? {
            println!("{} -> {}", l.name, l.version.to_string());
        }
    }
    Ok(())
}
