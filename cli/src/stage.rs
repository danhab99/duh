use std::error::Error;
use std::io::{self, Write};

use clap::clap_derive::Args;
use lib::{dedup::DedupProgress, diff::DiffFragment, repo::Repo};

#[derive(Args)]
#[command(about = "Stage a file (produce fragment objects without committing)")]
pub struct StageCommand {
    /// Path to the file to stage
    #[arg(help = "Path to the file to stage (relative to current working directory)")]
    pub file_path: String,
}

// Rainbow palette via ANSI 256-color indices: red → orange → yellow → green → cyan → blue → violet
const RAINBOW: &[u8] = &[196, 202, 208, 214, 220, 118, 46, 51, 27, 21, 93, 201];
const MAX_COLS: u64 = 60;

fn rainbow_block(col: u64) -> String {
    crate::colors::wrap(&format!("38;5;{}", RAINBOW[col as usize % RAINBOW.len()]), "=")
}

fn white_block() -> String {
    crate::colors::wrap("97", "=")
}

fn generate_bar_segment(width: u64, symbol: char, color: &str) -> String {
    let mut s = String::new();
    for _ in 0..width {
        s.push(symbol);
    }
    crate::colors::wrap(color, &s)
}

struct ProgressPrinter {
    bytes_per_col: u64,
    old_started: bool,
    old_bytes: u64,
    old_cols: u64,
    new_started: bool,
    new_bytes: u64,
    new_cols: u64,
}

impl ProgressPrinter {
    fn new(bytes_per_col: u64) -> Self {
        Self {
            bytes_per_col,
            old_started: false,
            old_bytes: 0,
            old_cols: 0,
            new_started: false,
            new_bytes: 0,
            new_cols: 0,
        }
    }

    fn on_event(&mut self, event: DedupProgress) {
        match event {
            DedupProgress::OldChunk { index: _, len } => {
                if !self.old_started {
                    print!("Old [");
                    self.old_started = true;
                }
                self.old_bytes += len as u64;
                let target = (self.old_bytes / self.bytes_per_col).min(MAX_COLS);
                while self.old_cols < target {
                    // Color by column position — always cycles through the full rainbow
                    // regardless of how many CDC chunks the file produces.
                    print!("{}", rainbow_block(self.old_cols));
                    self.old_cols += 1;
                }
                let _ = io::stdout().flush();
            }
            DedupProgress::NewChunk { index: _, len, old_index } => {
                if !self.new_started {
                    if self.old_started {
                        println!("]");
                    }
                    print!("New [");
                    self.new_started = true;
                }
                self.new_bytes += len as u64;
                let target = (self.new_bytes / self.bytes_per_col).min(MAX_COLS);
                while self.new_cols < target {
                    let block = if old_index.is_some() {
                        // Chunk exists in old stream: color by this column's position.
                        // Because matched content occupies the same byte range as in the
                        // old file, its columns naturally land at the same rainbow color.
                        rainbow_block(self.new_cols)
                    } else {
                        // Brand-new chunk (addition): white
                        white_block()
                    };
                    print!("{}", block);
                    self.new_cols += 1;
                }
                let _ = io::stdout().flush();
            }
        }
    }

    fn finish(&self) {
        if self.old_started || self.new_started {
            println!("]");
        }
    }
}

pub fn stage(repo: &mut Repo, cmd: &StageCommand) -> Result<(), Box<dyn Error>> {
    println!("{} {}", crate::colors::cyan("Staging file"), cmd.file_path);

    let file_size = std::fs::metadata(&cmd.file_path).map(|m| m.len()).unwrap_or(0);
    let bytes_per_col = (file_size / MAX_COLS).max(1);

    let mut printer = ProgressPrinter::new(bytes_per_col);
    let mut frag_events: Vec<(u64, char, &'static str)> = Vec::new();

    let h = repo.stage_file(
        cmd.file_path.clone(),
        Some(|fragment: DiffFragment| {
            let entry = match fragment {
                DiffFragment::ADDED { body } => (body.len() as u64, '+', "32"),
                DiffFragment::UNCHANGED { len } => (len as u64, '=', "37"),
                DiffFragment::DELETED { len } => (len as u64, '-', "31"),
            };
            frag_events.push(entry);
        }),
        Some(|event: DedupProgress| printer.on_event(event)),
    )?;

    printer.finish();

    // ── Diff bar ──────────────────────────────────────────────────────────
    let total_bytes: u64 = frag_events.iter().map(|(len, _, _)| *len).sum();
    let bar = if total_bytes == 0 {
        generate_bar_segment(MAX_COLS, '+', "32")
    } else {
        let bpc = (total_bytes / MAX_COLS).max(1);
        let mut s = String::new();
        let mut accumulated: u64 = 0;
        let mut emitted: u64 = 0;
        for (len, symbol, color) in &frag_events {
            accumulated += len;
            let target = (accumulated / bpc).min(MAX_COLS);
            let new_chars = target - emitted;
            emitted = target;
            if new_chars > 0 {
                s.push_str(&generate_bar_segment(new_chars, *symbol, *color));
            }
        }
        if emitted < MAX_COLS {
            if let Some((_, symbol, color)) = frag_events.last() {
                s.push_str(&generate_bar_segment(MAX_COLS - emitted, *symbol, *color));
            }
        }
        s
    };

    println!("Dif[{}]", bar);
    println!("{}", crate::colors::green(&h.to_string()));

    Ok(())
}
