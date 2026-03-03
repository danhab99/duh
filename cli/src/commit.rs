use std::{error::Error, fs};

use clap::clap_derive::Args;
use lib::repo::Repo;

#[derive(Args)]
#[command(about = "Stage the given file and create a commit with the provided message")]
pub struct CommitCommand {
    /// Optional path to the file to snapshot and include in the commit. If
    /// omitted the command will commit the current index (all staged files).
    #[arg(help = "Optional path to a file to stage and commit")]
    pub file_path: Option<String>,

    /// Commit message
    #[arg(
        short = 'm',
        long = "message",
        default_value = "Snapshot commit",
        help = "Message to store with the new commit"
    )]
    pub message: Option<String>,
}

pub fn commit(repo: &mut Repo, cmd: &CommitCommand) -> Result<(), Box<dyn Error>> {
    if let Some(fp) = &cmd.file_path {
        println!("{} {}", crate::colors::cyan("Staging file"), fp);
        repo.stage_file(fp.clone(), None::<fn(_)>)?;
    } else {
        println!(
            "{}",
            crate::colors::dim("No file provided — committing staged files in index")
        );
    }

    println!("{}", crate::colors::cyan("Committing"));
    let h = repo.commit(match cmd.message.clone() {
        Some(ref x) if !x.is_empty() && x.len() > 0 => x.clone(),
        _ => prompt_editor(),
    })?;
    println!(
        "{} {} — index cleared. Run `duh status` to inspect.",
        crate::colors::green(&h.to_string()),
        crate::colors::bold("Committed")
    );
    Ok(())
}

fn prompt_editor() -> String {
    let editor_command = std::env::var("EDITOR").unwrap();

    let message_dir = std::env::temp_dir().to_string_lossy().into_owned();
    let message_path = format!("{}/message", message_dir);

    let mut command = std::process::Command::new(editor_command);
    command.arg(message_path.clone());

    let exit_code = command.status().unwrap();

    if !exit_code.success() {
        panic!("exit code is not zero");
    }

    let msg = fs::read(message_path.clone()).unwrap();

    String::from_utf8(msg).unwrap()
}
