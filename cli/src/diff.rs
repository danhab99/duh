use std::error::Error;

use clap::clap_derive::Args;
use lib::display;
use lib::ops::compare::{compare, Difference};
use lib::ops::{fmt_bytes, look::look, strip_totals};
use lib::space::Space;

#[derive(Args)]
#[command(about = "Compare your files with the last save, or your picks with --picked")]
pub struct DiffCommand {
    /// Limit to these files (relative to the current directory)
    pub paths: Vec<String>,

    /// Compare what you've picked with the last save, instead of your
    /// un-picked edits
    #[arg(long = "picked", aliases = ["staged", "cached"])]
    pub staged: bool,
}

pub fn diff(space: &mut Space, cmd: &DiffCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let mut only = Vec::new();
    for p in &cmd.paths {
        only.push(space.repo_path_from(&cwd, p)?);
    }
    let report = look(space)?;
    for c in compare(space, &report, &only, cmd.staged)? {
        let label = match c.difference {
            Difference::Changed => "",
            Difference::New => " (new)",
            Difference::Deleted => " (deleted)",
            Difference::Missing => " (missing from your files)",
        };
        println!("{}{}", crate::colors::bold(&space.display_path(&cwd, &c.path)), label);
        if c.strip.is_empty() {
            continue;
        }
        let entries: Vec<_> = c.strip.iter().map(display::span_to_entry).collect();
        println!("{}", display::format_diff_bar(&display::generate_diff_bar(&entries)));
        let (added, removed, kept) = strip_totals(&c.strip);
        println!(
            "  {}  {}  {}",
            crate::colors::green(&format!("+{}", fmt_bytes(added))),
            crate::colors::red(&format!("-{}", fmt_bytes(removed))),
            crate::colors::dim(&format!("={}", fmt_bytes(kept)))
        );
    }
    Ok(())
}
