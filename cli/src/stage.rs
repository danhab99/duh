use std::{error::Error, io::Write, os::unix::fs::MetadataExt};

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

    let total_file_size = std::fs::metadata(cmd.file_path.clone())?.size();
    const MAX_WIDTH: u64 = 50;

    // bytes per bar character; clamp to 1 so we never divide by zero on tiny files
    let bytes_per_col = (total_file_size / MAX_WIDTH).max(1);

    let mut bar_display = String::from("");

    let h = repo.stage_file(
        cmd.file_path.clone(),
        Some(|fragment: DiffFragment| {
            let (next_len, symbol, color) = match fragment {
                lib::diff::DiffFragment::ADDED { body } => (body.len() as u64, '+', "32"),
                lib::diff::DiffFragment::UNCHANGED { len } => (len as u64, '=', "37"),
                lib::diff::DiffFragment::DELETED { len } => (len as u64, '-', "31"),
            };

            let display_width = next_len / bytes_per_col;

            let s = generate_bar_segment(display_width, symbol, color);
            bar_display.push_str(&s);
            print!("\r{}           ", bar_display);
            let _ = std::io::stdout().flush();

            return;
        }),
    )?;

    println!("{}", crate::colors::green(&h.to_string()));

    Ok(())
}
