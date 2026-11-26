use std::error::Error;
use std::path::Path;

use clap::clap_derive::Args;
use globwalk::GlobWalkerBuilder;
use lib::display::{self, ProgressPrinter};
use lib::ops::look::{look, Look};
use lib::ops::pick::{self, PickEvent, Picked};
use lib::ops::worktree;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Pick files to go into the next save")]
pub struct StageCommand {
    /// Files, folders or patterns like *.psd, relative to where you are
    #[arg(
        required_unless_present_any = ["all", "update"],
        help = "Files, folders or patterns like *.psd (relative to where you are)"
    )]
    pub file_paths: Vec<String>,

    /// Pick every change: new, changed and deleted files
    #[arg(short = 'A', long = "all", conflicts_with = "update")]
    pub all: bool,

    /// Pick changed and deleted tracked files only (no new files)
    #[arg(short = 'u', long = "update")]
    pub update: bool,
}

/// Prints picking as it happens: in a terminal, a progress bar while a file
/// is chunked; then its change bar and how much was added and removed.
pub struct PickPrinter {
    printer: Option<ProgressPrinter>,
}

impl PickPrinter {
    pub fn new() -> Self {
        Self { printer: None }
    }

    pub fn on(&mut self, event: PickEvent) {
        match event {
            PickEvent::Start { path, size } => {
                println!("-- Picking {}", path);
                // The live bars redraw as they go, which only makes sense
                // in a terminal; logs and pipes get the summary alone.
                if crate::colors::enabled() {
                    self.printer = Some(ProgressPrinter::new((size / display::MAX_COLS).max(1)));
                }
            }
            PickEvent::Chunk(e) => {
                if let Some(p) = self.printer.as_mut() {
                    p.on_event(e);
                }
            }
            PickEvent::Done(picked) => match picked {
                Picked::Changed { strip, .. } => {
                    if let Some(p) = self.printer.take() {
                        p.finish();
                    }
                    crate::diff::print_strip(&strip);
                }
                Picked::Unchanged { path } => {
                    self.printer = None;
                    println!("{}", crate::colors::dim(&format!("{} hasn't changed since the last save", path)));
                }
                Picked::Deletion { path } => println!("-- Picking the deletion of {}", path),
            },
        }
    }
}

fn pick_under(space: &mut Space, report: &Look, prefix: &str, tracked_only: bool) -> Result<(), Box<dyn Error>> {
    let mut out = PickPrinter::new();
    if pick::pick_under(space, report, prefix, tracked_only, |e| out.on(e))?.is_empty() {
        println!("{}", crate::colors::dim("Nothing to pick"));
    }
    Ok(())
}

pub fn stage(space: &mut Space, cmd: &StageCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let report = look(space)?;

    if cmd.file_paths.is_empty() {
        return pick_under(space, &report, "", cmd.update);
    }

    let tracked = report.tracked();
    for pattern in &cmd.file_paths {
        let path = Path::new(pattern);
        if path.is_file() {
            let repo_path = space.repo_path_from(&cwd, pattern)?;
            let mut out = PickPrinter::new();
            pick::pick_file(space, &repo_path, |e| out.on(e))?;
            continue;
        }
        println!("{} {}", crate::colors::cyan("Picking"), pattern);

        if path.is_dir() {
            let root = space.get_path_in_worktree("")?.canonicalize()?;
            let prefix = if cwd.join(path).canonicalize()? == root {
                String::new() // the space's root itself, e.g. `duh pick .`
            } else {
                space.repo_path_from(&cwd, pattern)?
            };
            pick_under(space, &report, &prefix, cmd.update)?;
            continue;
        }

        // A tracked file that's gone from disk: pick its deletion, like git.
        if let Ok(repo_path) = space.repo_path_from(&cwd, pattern) {
            if tracked.binary_search(&repo_path).is_ok() {
                pick::pick_deletion(space, &repo_path)?;
                println!("-- Picking the deletion of {}", repo_path);
                continue;
            }
        }

        // Otherwise a glob relative to the current directory.
        let mut patterns = vec![pattern.clone()];
        patterns.extend(worktree::ignore_patterns(space)?);
        let mut matched = 0usize;
        for entry in GlobWalkerBuilder::from_patterns(".", &patterns).build()? {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let repo_path = space.repo_path_from(&cwd, &entry.path().to_string_lossy())?;
            if worktree::is_hidden(&repo_path) {
                continue;
            }
            let mut out = PickPrinter::new();
            pick::pick_file(space, &repo_path, |e| out.on(e))?;
            matched += 1;
        }
        if matched == 0 {
            return Err(format!("no files match '{}'", pattern).into());
        }
    }
    Ok(())
}
