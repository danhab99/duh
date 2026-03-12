use std::error::Error;

use clap::clap_derive::Args;
use lib::{diff::DiffFragment, repo::Repo};

use crate::colors::wrap;

#[derive(Args)]
#[command(about = "Stage a file (produce fragment objects without committing)")]
pub struct StageCommand {
    /// Path to the file to stage
    #[arg(help = "Path to the file to stage (relative to current working directory)")]
    pub file_path: String,
}

fn generate_bar_segment(width: u64, symbol: char, color: &str) -> String {
    let mut s = String::new();
    for _ in 0..width {
        s.push(symbol);
    }
    wrap(color, &s)
}

pub fn stage(repo: &mut Repo, cmd: &StageCommand) -> Result<(), Box<dyn Error>> {
    println!("{} {}", crate::colors::cyan("Staging file"), cmd.file_path);

    const MAX_WIDTH: u64 = 50;

    // Collect all diff fragment events first so we can scale the bar to the
    // actual bytes the CDC reports (which may be far fewer than the file size
    // due to content-defined deduplication of identical chunks).
    let mut frag_events: Vec<(u64, char, &'static str)> = Vec::new();

    let h = repo.stage_file(
        cmd.file_path.clone(),
        Some(|fragment: DiffFragment| {
            let entry = match fragment {
                lib::diff::DiffFragment::ADDED { body } => (body.len() as u64, '+', "32"),
                lib::diff::DiffFragment::UNCHANGED { len } => (len as u64, '=', "37"),
                lib::diff::DiffFragment::DELETED { len } => (len as u64, '-', "31"),
            };
            frag_events.push(entry);
        }),
    )?;

    let total_bytes: u64 = frag_events.iter().map(|(len, _, _)| *len).sum();

    let bar = if total_bytes == 0 {
        // CDC found no unique chunk boundaries (uniform/repetitive content).
        // Fall back to a solid green bar indicating new content was staged.
        generate_bar_segment(MAX_WIDTH, '+', "32")
    } else {
        // Scale so that the full range of CDC-reported bytes fills MAX_WIDTH cols.
        let bytes_per_col = (total_bytes / MAX_WIDTH).max(1);
        let mut s = String::new();
        let mut accumulated: u64 = 0;
        let mut emitted: u64 = 0;

        for (len, symbol, color) in &frag_events {
            accumulated += len;
            let target = (accumulated / bytes_per_col).min(MAX_WIDTH);
            let new_chars = target - emitted;
            emitted = target;
            if new_chars > 0 {
                s.push_str(&generate_bar_segment(new_chars, *symbol, *color));
            }
        }

        // Pad to exactly MAX_WIDTH (integer division can leave 1 col short).
        if emitted < MAX_WIDTH {
            if let Some((_, symbol, color)) = frag_events.last() {
                s.push_str(&generate_bar_segment(MAX_WIDTH - emitted, *symbol, *color));
            }
        }
        s
    };

    println!("{}", bar);
    println!("{}", crate::colors::green(&h.to_string()));

    Ok(())
}
