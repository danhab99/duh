use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::ObjectReference;
use lib::ops::refs::{rewind, Rewind};
use lib::space::Space;

#[derive(Args)]
#[command(about = "Move your draft back (or forward) to another version")]
#[command(long_about = "Move your draft back (or forward) to another version.\n\n\
By default your files stay exactly as they are and nothing is picked, so the \
undone saves show up as changes. --keep-picked also picks them, ready to save \
again; --discard makes your files match that version, throwing away unsaved work.")]
#[command(group(clap::ArgGroup::new("mode").args(["soft", "mixed", "hard"])))]
pub struct ResetCommand {
    /// Pick the undone changes, ready to save again
    #[arg(long = "keep-picked", alias = "soft")]
    pub soft: bool,

    /// Keep your files as they are and un-pick everything (the default)
    #[arg(long = "mixed", hide = true)]
    pub mixed: bool,

    /// Make your files match that version, discarding unsaved work
    #[arg(long = "discard", alias = "hard")]
    pub hard: bool,

    /// Version to go to: a label, short hash, HEAD~1, … (default: the last save)
    #[arg(value_name = "VERSION")]
    pub commit: Option<ObjectReference>,
}

pub fn reset(space: &mut Space, cmd: &ResetCommand) -> Result<(), Box<dyn Error>> {
    let mode = if cmd.soft {
        Rewind::KeepPicked
    } else if cmd.hard {
        Rewind::Discard
    } else {
        Rewind::KeepFiles
    };
    let target = rewind(space, cmd.commit.as_ref(), mode)?;
    let what = match mode {
        Rewind::KeepPicked => "the undone changes are picked",
        Rewind::KeepFiles => "your files are unchanged",
        Rewind::Discard => "your files match it",
    };
    println!("{} to {} ({})", crate::colors::green("Rewound"), crate::colors::cyan(&target.to_string()[..12]), what);
    Ok(())
}
