use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::{Object, ObjectReference};
use lib::space::Space;

/// Inspect a version
#[derive(Args)]
#[command(about = "Inspect one version: who saved it, when, and its files")]
pub struct ShowCommand {
    #[arg(value_name = "VERSION", help = "Version to inspect: a label, short hash, HEAD~1, … (default: the last save)")]
    pub commit: Option<ObjectReference>,
}

pub fn show(space: &mut Space, cmd: &ShowCommand) -> Result<(), Box<dyn Error>> {
    let head = space.resolve_ref_name(
        cmd.commit
            .clone()
            .unwrap_or(ObjectReference::Ref("HEAD".to_string())),
    )?;

    if head.is_zero() {
        println!("No saves yet");
        return Ok(());
    }

    match space.get_object(head)? {
        Some(Object::Commit(c)) => {
            println!("version {}", crate::colors::cyan(&head.to_string()));
            if !c.parent.is_zero() {
                println!("previous: {}", crate::colors::dim(&c.parent.to_string()));
            }
            println!(
                "saved by: {} <{}> {}",
                c.author.name, c.author.email, c.author.timestamp
            );
            println!("\n    {}\n", c.message);
            println!("{}", crate::colors::bold("files:"));
            let files = space.get_commit_files(head)?;
            for (path, h) in files.iter() {
                println!(
                    "  {} -> {}",
                    crate::colors::cyan(path),
                    crate::colors::green(&h.to_string())
                );
            }
        }
        _ => return Err(format!("{} isn't a version", head.to_string()).into()),
    }

    Ok(())
}
