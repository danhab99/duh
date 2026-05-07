use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::{Object, ObjectReference};
use lib::repo::Repo;

#[derive(Args)]
#[command(about = "Show commit history starting at HEAD")]
pub struct LogCommand {
    /// Maximum number of commits to show
    #[arg(short = 'n', long = "max", help = "Limit the number of commits displayed")]
    pub max: Option<usize>,

    #[arg(help = "commit to show")]
    pub commit: Option<ObjectReference>,
}

pub fn log<F: vfs::FileSystem>(repo: &mut Repo<F>, cmd: &LogCommand) -> Result<(), Box<dyn Error>> {
    let mut cur = repo.resolve_ref_name(
        cmd.commit
            .clone()
            .unwrap_or(ObjectReference::Ref("HEAD".to_string())),
    )?;

    if cur.is_zero() {
        println!("No commits yet");
        return Ok(());
    }

    let mut shown = 0usize;
    while !cur.is_zero() {
        match repo.get_object(cur)? {
            Some(Object::Commit(c)) => {
                println!("commit {}", crate::colors::cyan(&cur.to_string()));
                println!("Author: {} <{}> {}", c.author.name, c.author.email, c.author.timestamp);
                // show only first line of message as summary
                if let Some(summary) = c.message.lines().next() {
                    println!("    {}", summary);
                }
                println!();

                shown += 1;
                if let Some(max) = cmd.max {
                    if shown >= max {
                        break;
                    }
                }

                cur = c.parent;
            }
            _ => {
                println!("HEAD does not point to a commit");
                break;
            }
        }
    }

    Ok(())
}
