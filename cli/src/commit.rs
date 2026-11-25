use std::{error::Error, fs};

use clap::clap_derive::Args;
use lib::ops::{pick, save};
use lib::space::Space;

use crate::stage::PickPrinter;

#[derive(Args)]
#[command(about = "Save a new version of your picked files")]
pub struct CommitCommand {
    /// A file to pick and save in one go (otherwise saves what you picked)
    #[arg(help = "A file to pick and save in one go")]
    pub file_path: Option<String>,

    /// Commit message
    #[arg(short = 'm', long = "message", help = "What changed, in a sentence")]
    pub message: Option<String>,

    /// Write the message for me from what changed
    #[arg(short = 'g', long = "generate", help = "Write the message for me from what changed")]
    pub generate: bool,

    /// Pick every changed or deleted tracked file first
    #[arg(short = 'a', long = "all")]
    pub all: bool,

    /// Save even if nothing changed
    #[arg(long = "allow-empty")]
    pub allow_empty: bool,
}

pub fn commit(space: &mut Space, cmd: &CommitCommand) -> Result<(), Box<dyn Error>> {
    let mut out = PickPrinter::new();
    if let Some(fp) = &cmd.file_path {
        let repo_path = space.repo_path_from(&std::env::current_dir()?, fp)?;
        pick::pick_file(space, &repo_path, |e| out.on(e))?;
    }
    if cmd.all {
        save::pick_tracked(space, |e| out.on(e))?;
    }
    if space.index.is_empty() && !cmd.allow_empty {
        // Let save explain.
        save::save(space, "-", false)?;
    }

    let message = match cmd.message.clone() {
        Some(x) if !x.is_empty() => x,
        _ if cmd.generate || std::env::var("EDITOR").is_err() => {
            let cwd = std::env::current_dir()?;
            let display_space = space.clone();
            let msg = save::suggest_message(space, |p| display_space.display_path(&cwd, p))?;
            println!(
                "{} {}",
                crate::colors::dim("Generated message:"),
                crate::colors::bold(msg.lines().next().unwrap_or(""))
            );
            msg
        }
        _ => prompt_editor()?,
    };

    let saved = save::save(space, &message, cmd.allow_empty)?;
    println!(
        "[{} {}] {}",
        crate::colors::green(saved.draft.as_deref().unwrap_or("no draft")),
        crate::colors::cyan(&saved.version.to_string()[..12]),
        saved.message.lines().next().unwrap_or("")
    );
    Ok(())
}

/// The message from $EDITOR, written in a file of its own so two saves at
/// once can't collide.
fn prompt_editor() -> Result<String, Box<dyn Error>> {
    let editor = std::env::var("EDITOR")?;
    let file = tempfile::Builder::new().prefix("duh-message-").suffix(".txt").tempfile()?;
    let status = std::process::Command::new(editor).arg(file.path()).status()?;
    if !status.success() {
        return Err(Box::new(lib::error::DuhError::EditorExitedWithError(status.code().unwrap_or(-1))));
    }
    Ok(fs::read_to_string(file.path())?)
}
