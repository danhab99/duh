use std::io::{self, Write};

use crate::dedup::DedupProgress;
use crate::diff::DiffFragment;

const RAINBOW: &[u8] = &[196, 202, 208, 214, 220, 118, 46, 51, 27, 21, 93, 201];
pub const MAX_COLS: u64 = 60;

fn wrap(code: &str, s: &str) -> String {
    format!("\x1b[{}m{}\x1b[0m", code, s)
}

pub fn rainbow_block(col: u64) -> String {
    wrap(
        &format!("38;5;{}", RAINBOW[col as usize % RAINBOW.len()]),
        "=",
    )
}

pub fn white_block() -> String {
    wrap("97", "=")
}

pub fn generate_bar_segment(width: u64, symbol: char, color: &str) -> String {
    let mut s = String::new();
    for _ in 0..width {
        s.push(symbol);
    }
    wrap(color, &s)
}

pub struct ProgressPrinter {
    bytes_per_col: u64,
    old_started: bool,
    old_bytes: u64,
    old_cols: u64,
    new_started: bool,
    new_bytes: u64,
    new_cols: u64,
}

impl ProgressPrinter {
    pub fn new(bytes_per_col: u64) -> Self {
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

    pub fn on_event(&mut self, event: DedupProgress) {
        match event {
            DedupProgress::OldChunk { index: _, len } => {
                if !self.old_started {
                    print!("Old [");
                    self.old_started = true;
                }
                self.old_bytes += len as u64;
                let target = (self.old_bytes / self.bytes_per_col).min(MAX_COLS);
                while self.old_cols < target {
                    print!("{}", rainbow_block(self.old_cols));
                    self.old_cols += 1;
                }
                let _ = io::stdout().flush();
            }
            DedupProgress::NewChunk {
                index: _,
                len,
                old_index,
            } => {
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
                        rainbow_block(self.new_cols)
                    } else {
                        white_block()
                    };
                    print!("{}", block);
                    self.new_cols += 1;
                }
                let _ = io::stdout().flush();
            }
        }
    }

    pub fn finish(&self) {
        if self.old_started || self.new_started {
            println!("]");
        }
    }
}

pub fn generate_diff_bar(frag_events: &[(u64, char, &'static str)]) -> String {
    let total_bytes: u64 = frag_events.iter().map(|(len, _, _)| *len).sum();
    if total_bytes == 0 {
        return generate_bar_segment(MAX_COLS, '+', "32");
    }

    // Round each fragment to columns on its own (fragments under half a column
    // vanish), then give the rounding slack to the largest fragment so the bar
    // is exactly MAX_COLS wide.
    let mut widths: Vec<i64> = frag_events
        .iter()
        .map(|(len, _, _)| {
            ((*len as u128 * MAX_COLS as u128 + total_bytes as u128 / 2) / total_bytes as u128)
                as i64
        })
        .collect();
    let slack = MAX_COLS as i64 - widths.iter().sum::<i64>();
    if let Some((largest, _)) = frag_events.iter().enumerate().max_by_key(|(_, (len, _, _))| *len) {
        widths[largest] = (widths[largest] + slack).max(0);
    }

    let mut s = String::new();
    for ((_, symbol, color), width) in frag_events.iter().zip(widths) {
        if width > 0 {
            s.push_str(&generate_bar_segment(width as u64, *symbol, *color));
        }
    }
    s
}

pub fn format_diff_bar(bar: &str) -> String {
    format!("Dif[{}]", bar)
}

pub fn fragment_to_entry(fragment: DiffFragment) -> (u64, char, &'static str) {
    match fragment {
        DiffFragment::ADDED { body } => (body.len() as u64, '+', "32"),
        DiffFragment::UNCHANGED { len } => (len as u64, '=', "37"),
        DiffFragment::DELETED { len } => (len as u64, '-', "31"),
    }
}
