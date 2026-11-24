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
#[command(about = "Move your draft back (or forward) to another version")]
#[command(long_about = "Move your draft back (or forward) to another version.\n\n\
By default your files stay exactly as they are and nothing is picked, so the \
undone saves show up as changes. --keep-picked also picks them, ready to save \
again; --discard makes your files match that version, throwing away unsaved work.")]
#[command(group(clap::ArgGroup::new("mode").args(["soft", "mixed", "hard"])))]
pub struct ResetCommand {
    /// Pick the undone changes, ready to save again
    #[arg(long = "keep-picked", alias = "soft")]
    pub soft: bool,

    /// Keep your files as they are and un-pick everything (the default)
    #[arg(long = "mixed", hide = true)]
    pub mixed: bool,

    /// Make your files match that version, discarding unsaved work
    #[arg(long = "discard", alias = "hard")]
    pub hard: bool,

    /// Version to go to: a label, short hash, HEAD~1, … (default: the last save)
    #[arg(value_name = "VERSION")]
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
        return Err(format!("{} isn't a version", target.to_string()).into());
    }

    if matches!(cmd.mode(), ResetMode::Soft) && !space.index.is_empty() {
        // Staged versions are diffs against the old HEAD; they can't be
        // carried onto a different parent.
        return Err("--keep-picked needs nothing picked yet; save or un-pick first".into());
    }

    // Move whatever HEAD points at: the current branch, or HEAD if detached.
    let msg = format!("rewind to {}", target.to_string());
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

    let what = match cmd.mode() {
        ResetMode::Soft => "the undone changes are picked",
        ResetMode::Mixed => "your files are unchanged",
        ResetMode::Hard => "your files match it",
    };
    println!(
        "{} to {} ({})",
        crate::colors::green("Rewound"),
        crate::colors::cyan(&target.to_string()[..12]),
        what
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
