use std::error::Error;

use clap::clap_derive::Args;
use lib::file::FileOps;
use lib::objects::{Object, ObjectReference};
use lib::space::Space;

use crate::checkout::checkout_tree;

#[derive(Clone, Copy, Debug)]
pub enum ResetMode {
    /// Move the branch only; the old commits' changes become staged.
    Soft,
    /// Move the branch and clear the index; the working tree is untouched.
    Mixed,
    /// Move the branch, clear the index and make the working tree match.
    Hard,
}

#[derive(Args)]
#[command(about = "Reset the current branch to a specified commit")]
#[command(group(clap::ArgGroup::new("mode").args(["soft", "mixed", "hard"])))]
pub struct ResetCommand {
    /// Keep the index and working tree; the undone changes become staged
    #[arg(long)]
    pub soft: bool,

    /// Clear the index, keep the working tree (default)
    #[arg(long)]
    pub mixed: bool,

    /// Discard the index and make the working tree match the commit
    #[arg(long)]
    pub hard: bool,

    /// Commit to reset to. Defaults to HEAD.
    #[arg(help = "Commit hash or ref to reset to (defaults to HEAD)")]
    pub commit: Option<ObjectReference>,
}

impl ResetCommand {
    fn mode(&self) -> ResetMode {
        if self.soft {
            ResetMode::Soft
        } else if self.hard {
            ResetMode::Hard
        } else {
            ResetMode::Mixed
        }
    }
}

pub fn reset(space: &mut Space, cmd: &ResetCommand) -> Result<(), Box<dyn Error>> {
    let old_head = space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;
    let target = match &cmd.commit {
        Some(r) => space.resolve_ref_name(r.clone())?,
        None => old_head,
    };
    if !matches!(space.get_object(target)?, Some(Object::Commit(_))) {
        return Err(format!("{} is not a commit", target.to_string()).into());
    }

    if matches!(cmd.mode(), ResetMode::Soft) && !space.index.is_empty() {
        // Staged versions are diffs against the old HEAD; they can't be
        // carried onto a different parent.
        return Err("soft reset needs an empty index; commit or unstage first".into());
    }

    // Move whatever HEAD points at: the current branch, or HEAD if detached.
    let msg = format!("reset: moving to {}", target.to_string());
    match space.get_ref("HEAD".to_string())? {
        ObjectReference::Ref(branch) => {
            space.set_ref(&branch, ObjectReference::Hash(target), Some(&msg))?
        }
        _ => space.set_ref("HEAD", ObjectReference::Hash(target), Some(&msg))?,
    }

    match cmd.mode() {
        ResetMode::Soft => stage_differences(space, old_head, target)?,
        ResetMode::Mixed => space.index.clear(),
        ResetMode::Hard => {
            space.index.clear();
            checkout_tree(space, old_head, target)?;
        }
    }

    println!(
        "{} to {} ({:?})",
        crate::colors::green("Reset"),
        crate::colors::cyan(&target.to_string()),
        cmd.mode()
    );
    Ok(())
}

/// After HEAD moved from `old_head` to `target`, stage every difference
/// (changed, added and removed files) so committing reproduces `old_head`'s
/// tree.
fn stage_differences(
    space: &mut Space,
    old_head: lib::hash::Hash,
    target: lib::hash::Hash,
) -> Result<(), Box<dyn Error>> {
    if old_head.is_zero() {
        return Ok(());
    }
    let old_files = space.get_commit_files(old_head)?;
    let new_files = space.get_commit_files(target)?;

    let mut paths: Vec<&String> = old_files
        .iter()
        .filter(|(p, v)| new_files.get(*p) != Some(v))
        .map(|(p, _)| p)
        .collect();
    paths.sort();

    let mut fileops = FileOps::from_space(space);
    for path in paths {
        let content = fileops.open_file(path.clone(), old_head)?;
        fileops.stage_reader(path.clone(), content, None::<fn(_)>, None::<fn(_)>)?;
    }
    // Files the target has but the old HEAD didn't: stage their removal.
    for path in new_files.keys().filter(|p| !old_files.contains_key(*p)) {
        fileops.stage_deletion(path.clone())?;
    }
    Ok(())
}
