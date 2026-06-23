use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::{Object, ObjectReference};
use lib::space::Space;

/// Show the commit currently referenced by HEAD
#[derive(Args)]
#[command(about = "Display the HEAD commit (hash, author, message, files)")]
pub struct ShowCommand {
    #[arg(help = "commit to show")]
    pub commit: Option<ObjectReference>,
}

pub fn show<F: vfs::FileSystem>(space: &mut Space<F>, cmd: &ShowCommand) -> Result<(), Box<dyn Error>> {
    let head = space.resolve_ref_name(
        cmd.commit
            .clone()
            .unwrap_or(ObjectReference::Ref("HEAD".to_string())),
    )?;

    if head.is_zero() {
        println!("No commits yet");
        return Ok(());
    }

    match space.get_object(head)? {
        Some(Object::Commit(c)) => {
            println!("{}", crate::colors::cyan(&head.to_string()));
            println!("parent: {}", crate::colors::dim(&c.parent.to_string()));
            println!(
                "author: {} <{}> {}",
                c.author.name, c.author.email, c.author.timestamp
            );
            println!(
                "committer: {} <{}> {}",
                c.comitter.name, c.comitter.email, c.comitter.timestamp
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
        _ => println!("HEAD does not point to a commit"),
    }

    Ok(())
}
