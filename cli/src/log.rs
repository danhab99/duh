use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::{Object, ObjectReference};
use lib::space::Space;

#[derive(Args)]
#[command(about = "Show your saves, newest first")]
pub struct LogCommand {
    /// Maximum number of commits to show
    #[arg(short = 'n', long = "max", help = "Show at most this many saves")]
    pub max: Option<usize>,

    #[arg(value_name = "VERSION", help = "Start from this version instead of the last save")]
    pub commit: Option<ObjectReference>,

    /// One line per save: short hash and message
    #[arg(long = "oneline")]
    pub oneline: bool,
}

pub fn log(space: &mut Space, cmd: &LogCommand) -> Result<(), Box<dyn Error>> {
    let mut cur = space.resolve_ref_name(
        cmd.commit
            .clone()
            .unwrap_or(ObjectReference::Ref("HEAD".to_string())),
    )?;

    if cur.is_zero() {
        println!("No saves yet");
        return Ok(());
    }

    let mut shown = 0usize;
    while !cur.is_zero() {
        match space.get_object(cur)? {
            Some(Object::Commit(c)) => {
                if cmd.oneline {
                    println!(
                        "{} {}",
                        crate::colors::cyan(&cur.to_string()[..12]),
                        c.message.lines().next().unwrap_or("")
                    );
                } else {
                    println!("version {}", crate::colors::cyan(&cur.to_string()));
                    println!("Saved by: {} <{}> {}", c.author.name, c.author.email, c.author.timestamp);
                    println!();
                    for line in c.message.lines() {
                        println!("    {}", line);
                    }
                    println!();
                }

                shown += 1;
                if let Some(max) = cmd.max {
                    if shown >= max {
                        break;
                    }
                }

                cur = c.parent;
            }
            _ => {
                println!("{} isn't a version", cur.to_string());
                break;
            }
        }
    }

    Ok(())
}
