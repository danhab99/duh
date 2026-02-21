use std::{collections::HashMap, error::Error};

use clap::clap_derive::Args;
use lib::{
    hash::Hash,
    objects::{Object, ObjectReference},
    repo::Repo,
};

#[derive(Args)]
#[command(about = "Show status of tracked files (compare working copy -> index / HEAD)")]
pub struct StatusCommand {}

pub fn status(repo: &mut Repo, _cmd: &StatusCommand) -> Result<bool, Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let cwd_str = cwd.to_str().unwrap_or("").to_string();

    // Build the set of tracked files using HEAD as the base and letting staged
    // entries in the index override when present.
    let mut tracked: HashMap<String, Hash> = HashMap::new();

    let head_hash = repo.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;
    if !head_hash.is_zero() {
        if let Some(Object::Commit(head)) = repo.get_object(head_hash)? {
            for (path, version_hash) in head.files.iter() {
                tracked.insert(path.clone(), *version_hash);
            }
        }
    }

    for path in repo.index_paths() {
        if let Some(version_hash) = repo.get_indexed_version(&path) {
            tracked.insert(path.clone(), version_hash);
        }
    }

    let mut changed: Vec<String> = Vec::new();
    let mut unchanged: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    for (path, version_hash) in tracked.iter() {
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

        match repo.get_object(*version_hash)? {
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

    let mut uncommitted_changes = false;

    if !changed.is_empty() {
        println!("{}", crate::colors::yellow("Modified files:"));
        for p in &changed {
            println!("  {}", crate::colors::yellow(p));
        }
        uncommitted_changes = true;
    }
    if !missing.is_empty() {
        println!("{}", crate::colors::red("Deleted / missing files:"));
        for p in &missing {
            println!("  {}", crate::colors::red(p));
        }
        uncommitted_changes = true;
    }

    if !unchanged.is_empty() {
        println!("{}", crate::colors::green("Staged / up-to-date files:"));
        for p in &unchanged {
            println!("  {}", crate::colors::green(p));
        }
        uncommitted_changes = true;
    }

    if changed.is_empty() && missing.is_empty() && unchanged.is_empty() {
        println!(
            "{}",
            crate::colors::dim(
                "No tracked files in index or HEAD. Use `duh stage <file>` to add files."
            )
        );
    }

    Ok(uncommitted_changes)
}
