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
        help = "Message to store with the new commit"
    )]
    pub message: Option<String>,

    /// Auto-generate a commit message from the staged diff
    #[arg(
        short = 'g',
        long = "generate",
        help = "Generate a commit message from staged changes instead of prompting"
    )]
    pub generate: bool,
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
    let message = match cmd.message.clone() {
        Some(ref x) if !x.is_empty() => x.clone(),
        _ if cmd.generate || std::env::var("EDITOR").is_err() => {
            let msg = generate_message(repo)?;
            println!("{} {}", crate::colors::dim("Generated message:"), crate::colors::bold(&msg.lines().next().unwrap_or("")));
            msg
        }
        _ => prompt_editor(),
    };
    let h = repo.commit(message)?;
    println!(
        "{} {} — index cleared. Run `duh status` to inspect.",
        crate::colors::green(&h.to_string()),
        crate::colors::bold("Committed")
    );
    Ok(())
}

fn generate_message(repo: &mut Repo) -> Result<String, Box<dyn Error>> {
    let summaries = repo.staged_summary()?;
    if summaries.is_empty() {
        return Ok("Empty commit".to_string());
    }

    let cwd = std::env::current_dir()?;
    let cwd_str = format!("{}/", cwd.to_str().unwrap_or(""));

    let file_count = summaries.len();
    let noun = if file_count == 1 { "file" } else { "files" };

    let mut lines = vec![format!("Update {} {}", file_count, noun), String::new()];

    let mut sorted = summaries;
    sorted.sort_by(|a, b| a.path.cmp(&b.path));

    for s in &sorted {
        let display = s.path.strip_prefix(&cwd_str).unwrap_or(&s.path);
        let is_new = s.unchanged_bytes == 0 && s.deleted_bytes == 0;
        if is_new {
            lines.push(format!("  {}  +{}  (new)", display, fmt_bytes(s.added_bytes)));
        } else {
            let mut parts = vec![format!("+{}", fmt_bytes(s.added_bytes))];
            if s.deleted_bytes > 0 {
                parts.push(format!("-{}", fmt_bytes(s.deleted_bytes)));
            }
            parts.push(format!("~{} unchanged", fmt_bytes(s.unchanged_bytes)));
            lines.push(format!("  {}  {}", display, parts.join("  ")));
        }
    }

    Ok(lines.join("\n"))
}

fn fmt_bytes(n: usize) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1} GB", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.1} MB", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1} KB", n as f64 / 1_000.0)
    } else {
        format!("{} B", n)
    }
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
