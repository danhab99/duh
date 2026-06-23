use std::error::Error;

use clap::clap_derive::Args;
use globwalk::GlobWalkerBuilder;
use lib::display::{self, ProgressPrinter};
use lib::file::FileOps;
use lib::{dedup::DedupProgress, diff::DiffFragment, space::Space};

#[derive(Args)]
#[command(about = "Stage files (produce fragment objects without committing)")]
pub struct StageCommand {
    /// Paths or globs of files to stage
    #[arg(
        required = true,
        help = "Paths or glob patterns of files to stage (relative to current working directory)"
    )]
    pub file_paths: Vec<String>,
}

fn stage_file(
    file_path: &str,
    space: &mut Space,
) -> Result<(), Box<dyn Error>> {
    let file_size = std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);
    let bytes_per_col = (file_size / display::MAX_COLS).max(1);

    let mut printer = ProgressPrinter::new(bytes_per_col);
    let mut frag_events: Vec<(u64, char, &'static str)> = Vec::new();

    let mut fileops = FileOps::from_space(space);

    let h = fileops.stage_file(
        file_path.to_string(),
        Some(|fragment: DiffFragment| {
            frag_events.push(display::fragment_to_entry(fragment));
        }),
        Some(|event: DedupProgress| printer.on_event(event)),
    )?;

    printer.finish();

    let bar = display::generate_diff_bar(&frag_events);
    println!("{}", display::format_diff_bar(&bar));
    println!("{}", crate::colors::green(&h.to_string()));

    Ok(())
}

pub fn stage(
    space: &mut Space,
    cmd: &StageCommand,
) -> Result<(), Box<dyn Error>> {
    // Load .duhignore from the space root if it exists. Each non-blank,
    // non-comment line becomes a negated glob pattern passed to GlobWalkerBuilder.
    let ignore_path = std::env::current_dir()?.join(".duhignore");
    let mut ignore_patterns: Vec<String> = Vec::new();
    if ignore_path.exists() {
        let contents = std::fs::read_to_string(&ignore_path)?;
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Negated entries in .duhignore are re-includes; everything else is excluded.
            if line.starts_with('!') {
                ignore_patterns.push(line[1..].to_string());
            } else {
                ignore_patterns.push(format!("!{}", line));
            }
        }
    }

    for pattern in &cmd.file_paths {
        println!("{} {}", crate::colors::cyan("Staging"), pattern);

        // Determine walk base and the glob pattern relative to it.
        let (base, main_pattern) = if std::path::Path::new(pattern).is_dir() {
            (pattern.as_str(), "**/*".to_string())
        } else {
            (".", pattern.clone())
        };

        // Build the pattern list: the main glob + any ignore exclusions.
        let mut patterns: Vec<String> = vec![main_pattern];
        patterns.extend(ignore_patterns.iter().cloned());

        let walker = GlobWalkerBuilder::from_patterns(base, &patterns).build()?;

        for f in walker {
            if let Ok(file) = f {
                let p = file.path();
                // Skip directories and hidden files/dirs (any component starting with '.').
                if p.is_dir() {
                    continue;
                }
                let hidden = p.components().any(|c| {
                    let s = c.as_os_str().to_string_lossy();
                    s.starts_with('.') && s != "."
                });
                if hidden {
                    continue;
                }
                let path = p.to_string_lossy().into_owned();
                println!("-- Staging file {}", path);
                stage_file(&path, space)?;
            }
        }
    }

    Ok(())
}
