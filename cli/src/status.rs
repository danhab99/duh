use std::{collections::HashMap, error::Error};

use clap::clap_derive::Args;
use lib::{
    hash::Hash,
    objects::{Object, ObjectReference},
    space::Space,
};

#[derive(Args)]
#[command(about = "Show status of tracked files (compare working copy -> index / HEAD)")]
pub struct StatusCommand {}

pub fn status(space: &mut Space, _cmd: &StatusCommand) -> Result<bool, Box<dyn Error>> {
    if space.worktree.is_none() {
        return Err("status needs a working tree (this space is bare)".into());
    }
    let cwd = std::env::current_dir()?;

    // Build the set of tracked files using HEAD as the base and letting staged
    // entries in the index override when present.
    let mut tracked: HashMap<String, Hash> = HashMap::new();

    let head_hash = space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;
    if !head_hash.is_zero() {
        if let Some(Object::Commit(_head)) = space.get_object(head_hash)? {
            let head_files = space.get_commit_files(head_hash)?;
            for (path, version_hash) in head_files.iter() {
                tracked.insert(path.clone(), *version_hash);
            }
        }
    }

    for path in space.index_paths() {
        if let Some(version_hash) = space.get_indexed_version(&path) {
            tracked.insert(path.clone(), version_hash);
        }
    }

    let mut changed: Vec<String> = Vec::new();
    let mut unchanged: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    for (path, version_hash) in tracked.iter() {
        let display = space.display_path(&cwd, path);

        let full_path = space.get_path_in_worktree(path)?;
        if !full_path.exists() {
            missing.push(display);
            continue;
        }

        let mut f = std::fs::File::open(&full_path)?;
        let working_hash = Hash::digest_file_stream(&mut f)?;

        match space.get_object(*version_hash)? {
            Some(Object::File(fv)) => {
                if fv.content_hash == working_hash {
                    unchanged.push(display);
                } else {
                    changed.push(display);
                }
            }
            _ => changed.push(display),
        }
    }

    changed.sort();
    unchanged.sort();
    missing.sort();

    let head_ref_name = match space.get_ref("HEAD".into())? {
        ObjectReference::Ref(name) => name,
        ObjectReference::Hash(hash) => hash.to_string(),
        ObjectReference::AbbrevHash(s) => s,
    };

    println!("On branch {}", crate::colors::green(head_ref_name.as_str()));
    println!("");

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
