use std::error::Error;
use std::path::Path;

use clap::clap_derive::Args;
use globwalk::GlobWalkerBuilder;
use lib::display::{self, ProgressPrinter};
use lib::file::FileOps;
use lib::{dedup::DedupProgress, diff::DiffFragment, space::Space};

use crate::status;

#[derive(Args)]
#[command(about = "Add file contents to the index (stage them for the next commit)")]
pub struct StageCommand {
    /// Files, directories or glob patterns, relative to the current directory
    #[arg(
        required_unless_present_any = ["all", "update"],
        help = "Files, directories or glob patterns to stage (relative to the current directory)"
    )]
    pub file_paths: Vec<String>,

    /// Stage every change in the worktree: new, modified and deleted files
    #[arg(short = 'A', long = "all", conflicts_with = "update")]
    pub all: bool,

    /// Stage modified and deleted tracked files only (no new files)
    #[arg(short = 'u', long = "update")]
    pub update: bool,
}

/// Stage one file by repo path, printing its diff bar. A file identical to
/// HEAD is reported as unchanged and not staged.
pub fn stage_repo_path(space: &mut Space, repo_path: &str) -> Result<(), Box<dyn Error>> {
    let full_path = space.get_path_in_worktree(repo_path)?;
    let file_size = std::fs::metadata(&full_path).map(|m| m.len()).unwrap_or(0);
    let bytes_per_col = (file_size / display::MAX_COLS).max(1);

    let mut printer = ProgressPrinter::new(bytes_per_col);
    let mut frag_events: Vec<(u64, char, &'static str)> = Vec::new();

    println!("-- Staging file {}", repo_path);
    let staged = FileOps::from_space(space).stage_file(
        repo_path.to_string(),
        Some(|fragment: DiffFragment| {
            frag_events.push(display::fragment_to_entry(fragment));
        }),
        Some(|event: DedupProgress| printer.on_event(event)),
    )?;

    match staged {
        Some(h) => {
            printer.finish();
            let bar = display::generate_diff_bar(&frag_events);
            println!("{}", display::format_diff_bar(&bar));
            println!("{}", crate::colors::green(&h.to_string()));
        }
        None => println!("{}", crate::colors::dim(&format!("{} is unchanged", repo_path))),
    }
    Ok(())
}

fn stage_deletion(space: &mut Space, repo_path: &str) -> Result<(), Box<dyn Error>> {
    FileOps::from_space(space).stage_deletion(repo_path.to_string())?;
    println!("-- Staging deletion of {}", repo_path);
    Ok(())
}

/// Stage every change under `prefix` ("" = whole worktree): modified files,
/// deletions of missing tracked files, and (unless `tracked_only`) new files.
fn stage_tree(
    space: &mut Space,
    report: &status::StatusReport,
    prefix: &str,
    tracked_only: bool,
) -> Result<usize, Box<dyn Error>> {
    let under = |p: &String| prefix.is_empty() || p.starts_with(&format!("{}/", prefix));
    let mut count = 0;
    for p in report.modified.iter().filter(|p| under(p)) {
        stage_repo_path(space, p)?;
        count += 1;
    }
    for p in report.missing.iter().filter(|p| under(p)) {
        stage_deletion(space, p)?;
        count += 1;
    }
    if !tracked_only {
        for p in report.untracked.iter().filter(|p| under(p)) {
            stage_repo_path(space, p)?;
            count += 1;
        }
    }
    Ok(count)
}

pub fn stage(space: &mut Space, cmd: &StageCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let report = status::collect(space)?;

    if cmd.file_paths.is_empty() {
        if stage_tree(space, &report, "", cmd.update)? == 0 {
            println!("{}", crate::colors::dim("Nothing to stage"));
        }
        return Ok(());
    }

    let tracked = report.tracked();
    for pattern in &cmd.file_paths {
        println!("{} {}", crate::colors::cyan("Staging"), pattern);
        let path = Path::new(pattern);

        if path.is_file() {
            let repo_path = space.repo_path_from(&cwd, pattern)?;
            stage_repo_path(space, &repo_path)?;
            continue;
        }

        if path.is_dir() {
            let root = space.get_path_in_worktree("")?.canonicalize()?;
            let prefix = if cwd.join(path).canonicalize()? == root {
                String::new() // the worktree root itself, e.g. `duh add .`
            } else {
                space.repo_path_from(&cwd, pattern)?
            };
            if stage_tree(space, &report, &prefix, cmd.update)? == 0 {
                println!("{}", crate::colors::dim("Nothing to stage"));
            }
            continue;
        }

        // A tracked file that's gone from disk: stage its deletion, like git.
        if let Ok(repo_path) = space.repo_path_from(&cwd, pattern) {
            if tracked.binary_search(&repo_path).is_ok() {
                stage_deletion(space, &repo_path)?;
                continue;
            }
        }

        // Otherwise a glob relative to the current directory.
        let mut patterns = vec![pattern.clone()];
        patterns.extend(crate::worktree::ignore_patterns(space)?);
        let mut matched = 0usize;
        for entry in GlobWalkerBuilder::from_patterns(".", &patterns).build()? {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let repo_path = space.repo_path_from(&cwd, &entry.path().to_string_lossy())?;
            if crate::worktree::is_hidden(&repo_path) {
                continue;
            }
            stage_repo_path(space, &repo_path)?;
            matched += 1;
        }
        if matched == 0 {
            return Err(format!("pathspec '{}' did not match any files", pattern).into());
        }
    }

    Ok(())
}
