use std::error::Error;
use std::fs;

use clap::clap_derive::Args;
use lib::dedup::{build_diff_fragments, DedupProgress};
use lib::display;
use lib::file::FileOps;
use lib::objects::Object;
use lib::space::{ReadSeek, Space};

use crate::commit::fmt_bytes;
use crate::status::{self, StagedChange};

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

type Entry = (u64, char, &'static str);

fn print_file(space: &Space, repo_path: &str, label: &str, entries: &[Entry]) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let sum = |c: char| entries.iter().filter(|e| e.1 == c).map(|e| e.0 as usize).sum::<usize>();
    println!("{}{}", crate::colors::bold(&space.display_path(&cwd, repo_path)), label);
    if entries.is_empty() {
        return;
    }
    println!("{}", display::format_diff_bar(&display::generate_diff_bar(entries)));
    println!(
        "  {}  {}  {}",
        crate::colors::green(&format!("+{}", fmt_bytes(sum('+')))),
        crate::colors::red(&format!("-{}", fmt_bytes(sum('-')))),
        crate::colors::dim(&format!("={}", fmt_bytes(sum('='))))
    );
}

/// Diff `old` → `new` in memory (nothing is stored).
fn compute(space: &Space, old: Box<dyn ReadSeek>, new: Box<dyn ReadSeek>) -> Result<Vec<Entry>, Box<dyn Error>> {
    let frags = build_diff_fragments(old, new, space.chunk_size, space.chunk_size as u64, space.max_chunk(), None::<fn(DedupProgress)>)?;
    Ok(frags.map(display::fragment_to_entry).collect())
}

pub fn diff(space: &mut Space, cmd: &DiffCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let mut only = Vec::new();
    for p in &cmd.paths {
        only.push(space.repo_path_from(&cwd, p)?);
    }
    let wanted = |p: &String| only.is_empty() || only.contains(p);
    let report = status::collect(space)?;

    if cmd.staged {
        for (path, change) in report.staged.iter().filter(|(p, _)| wanted(p)) {
            if *change == StagedChange::Deleted {
                print_file(space, path, " (deleted)", &[]);
                continue;
            }
            let version = match space.get_indexed_version(path).map(|h| space.get_object(h)) {
                Some(Ok(Some(Object::File(f)))) => f,
                _ => return Err(format!("picked version of '{}' is unreadable", path).into()),
            };
            let entries: Vec<Entry> = FileOps::from_space(space)
                .load_fragments(&version)?
                .iter()
                .map(display::file_fragment_to_entry)
                .collect();
            let label = if *change == StagedChange::New { " (new)" } else { "" };
            print_file(space, path, label, &entries);
        }
        return Ok(());
    }

    let head = space.get_head_commit_hash()?;
    for path in report.modified.iter().filter(|p| wanted(p)) {
        let staged = space.index.get(path).is_some_and(|h| !h.is_zero());
        let old = if staged {
            FileOps::from_space(space).open_staged(path.clone())?
        } else {
            FileOps::from_space(space).open_file(path.clone(), head)?
        };
        let new: Box<dyn ReadSeek> = Box::new(fs::File::open(space.get_path_in_worktree(path)?)?);
        let entries = compute(space, old, new)?;
        print_file(space, path, "", &entries);
    }
    for path in report.missing.iter().filter(|p| wanted(p)) {
        print_file(space, path, " (missing from your files)", &[]);
    }
    Ok(())
}
