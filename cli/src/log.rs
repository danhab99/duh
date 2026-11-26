use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::ObjectReference;
use lib::ops::history;
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
    let from = history::resolve(space, &cmd.commit.clone().unwrap_or(ObjectReference::Ref("HEAD".into())))?;
    if from.is_zero() {
        println!("No saves yet");
        return Ok(());
    }
    for s in history::history(space, from, cmd.max)? {
        if cmd.oneline {
            println!("{} {}", crate::colors::cyan(&s.version.to_string()[..12]), s.message.lines().next().unwrap_or(""));
            continue;
        }
        println!("version {}", crate::colors::cyan(&s.version.to_string()));
        println!("Saved by: {} {}", crate::when::person(&s.author_name, &s.author_email), crate::when::utc(s.time));
        println!();
        for line in s.message.lines() {
            println!("    {}", line);
        }
        println!();
    }
    Ok(())
}
