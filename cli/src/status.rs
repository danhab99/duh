use std::error::Error;

use clap::clap_derive::Args;
use lib::{hash::Hash, objects::Object, repo::Repo};

#[derive(Args)]
#[command(about = "Show status of tracked files (compare working copy -> index)")]
pub struct StatusCommand {}

pub fn status(repo: &mut Repo, _cmd: &StatusCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let cwd_str = cwd.to_str().unwrap_or("").to_string();

    let mut changed: Vec<String> = Vec::new();
    let mut unchanged: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    for path in repo.index_paths() {
        // make path nicer for display (relative to CWD when possible)
        let display = if path.starts_with(&cwd_str) {
            // skip leading separator if present
            let rel = path.strip_prefix(&format!("{}/", cwd_str)).unwrap_or(&path);
            rel.to_string()
        } else {
            path.clone()
        };

        let p = std::path::Path::new(&path);
        if !p.exists() {
            missing.push(display);
            continue;
        }

        let mut f = std::fs::File::open(&path)?;
        let working_hash = Hash::digest_file_stream(&mut f)?;

        let version_hash = match repo.get_indexed_version(&path) {
            Some(h) => h,
            None => {
                changed.push(display);
                continue;
            }
        };

        match repo.get_object(version_hash)? {
            Some(Object::FileVersion(fv)) => {
                if fv.content_hash == working_hash {
                    unchanged.push(display);
                } else {
                    changed.push(display);
                }
            }
            _ => changed.push(display),
        }
    }

    if !changed.is_empty() {
        println!("Modified files:");
        for p in &changed { println!("  {}", p); }
    }

    if !missing.is_empty() {
        println!("Deleted / missing files:");
        for p in &missing { println!("  {}", p); }
    }

    if !unchanged.is_empty() {
        println!("Staged / up-to-date files:");
        for p in &unchanged { println!("  {}", p); }
    }

    if changed.is_empty() && missing.is_empty() && unchanged.is_empty() {
        println!("Index is empty (no tracked files). Use `duh stage <file>` to add files.");
    }

    Ok(())
}
