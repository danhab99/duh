use std::error::Error;
use std::path::PathBuf;

use clap::clap_derive::Args;
use lib::{hash::Hash, objects::ObjectReference, space::Space};

use crate::checkout::restore;

#[derive(Args)]
#[command(about = "Copy commits to a remote (default) or from one (--from); fast-forward only")]
pub struct CopyCommand {
    #[arg(
        short = 'f',
        long = "from",
        help = "Remote to copy commits from (like git pull --ff-only)"
    )]
    pub from: Option<String>,

    #[arg(
        short = 't',
        long = "to",
        help = "Remote to copy commits to (like git push; default: origin)"
    )]
    pub to: Option<String>,

    #[arg(
        short = 'a',
        long = "as",
        help = "Local branch to update when copying from a remote (default: the current branch)"
    )]
    pub as_branch: Option<String>,

    #[arg(
        short = 'b',
        long = "branch",
        help = "Remote branch (default: the local branch's name)"
    )]
    pub branch: Option<String>,

    #[arg(
        long = "force",
        help = "When copying to a remote, overwrite its branch even if that discards commits"
    )]
    pub force: bool,
}

#[derive(clap::Args)]
#[command(about = "Clone a repository into a new directory")]
pub struct CloneCommand {
    /// Repository to clone: a path, file:// URL or other remote URL
    pub url: String,

    /// Directory to create (default: derived from the URL)
    pub directory: Option<String>,

    /// Branch to check out (default: the remote's current branch)
    #[arg(short = 'b', long = "branch")]
    pub branch: Option<String>,

    /// Clone the history without checking out a working tree
    #[arg(long = "bare")]
    pub bare: bool,

    /// Backend option or credential for the remote, saved in the new
    /// space's config (e.g. -o region=eu-west-1 -o access_key_id=…)
    #[arg(short = 'o', long = "option", value_name = "KEY=VALUE")]
    pub options: Vec<String>,
}

/// Directory name to clone `url` into: its last path component, ignoring
/// trailing slashes and a trailing `.duh` metadata directory or suffix.
pub fn default_clone_dir(url: &str) -> Result<String, Box<dyn Error>> {
    let name = url
        .trim_end_matches('/')
        .trim_end_matches(lib::utils::SPACE_METADATA_DIR_NAME)
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("")
        .trim_end_matches(".duh");
    if name.is_empty() || name == "." || name == ".." {
        return Err(format!(
            "can't infer a directory name from '{}'; pass one with --as",
            url
        )
        .into());
    }
    Ok(name.to_string())
}

/// Clone a remote repository into a new directory (git clone equivalent)
pub fn clone(
    url: &str,
    dest_dir: &str,
    remote_branch: Option<&str>,
    bare: bool,
    options: &[(String, String)],
) -> Result<(), Box<dyn Error>> {
    use std::fs;

    let dest_path = PathBuf::from(dest_dir);
    let metadata_path = dest_path.join(lib::utils::SPACE_METADATA_DIR_NAME);

    if dest_path.exists() {
        return Err(format!("destination '{}' already exists", dest_dir).into());
    }

    // Create the destination directory; remove it again if the clone fails
    // so a failed clone leaves nothing behind.
    fs::create_dir_all(&metadata_path)?;
    clone_into(url, &dest_path, &metadata_path, remote_branch, bare, options).map_err(|e| {
        let _ = fs::remove_dir_all(&dest_path);
        e
    })?;

    println!(
        "{} {} -> {}",
        crate::colors::cyan("Cloned"),
        crate::colors::green(url),
        crate::colors::green(dest_dir)
    );
    Ok(())
}

fn clone_into(
    url: &str,
    dest_path: &PathBuf,
    metadata_path: &PathBuf,
    remote_branch: Option<&str>,
    bare: bool,
    options: &[(String, String)],
) -> Result<(), Box<dyn Error>> {
    // Initialize a new duh space
    let worktree = if bare { None } else { Some(dest_path.clone()) };
    let mut space = Space::init(lib::store::local(metadata_path)?, worktree)?;

    // Add the remote as "origin"
    space.add_remote_with_options("origin", url, options)?;

    // Open the remote
    let remote = space.get_remote_by_name("origin")?;

    // Determine which branch to clone
    let head_name = remote
        .get_head_branch_name()
        .unwrap_or_else(|_| "HEAD".to_string());
    let target_branch = remote_branch.unwrap_or(&head_name);

    // Resolve the remote branch (an empty remote has none yet).
    let remote_hash = match remote.resolve_ref_name(ObjectReference::Ref(target_branch.to_string())) {
        Ok(h) => h,
        Err(_) if remote_branch.is_some() => {
            return Err(format!("remote has no branch '{}'", target_branch).into())
        }
        Err(_) => Hash::new(),
    };

    remote.copy_commit_to(&space, remote_hash)?;

    // Set up local branch tracking the remote
    let local_branch = target_branch;
    space.set_ref(
        local_branch,
        ObjectReference::Hash(remote_hash),
        Some(&format!("clone from {}: {}", "origin", local_branch)),
    )?;

    space.set_ref(
        "HEAD",
        ObjectReference::Ref(local_branch.to_string()),
        Some("clone from remote"),
    )?;

    if !bare {
        // Checkout all files
        let files = space.list_files(ObjectReference::Hash(remote_hash))?;

        for file_path in files {
            restore(&mut space, &file_path, remote_hash, None)?;
        }
    }

    space.save_index()?;

    Ok(())
}

pub fn copy(space: &mut Space, cmd: &CopyCommand) -> Result<(), Box<dyn Error>> {
    let head_branch = space.get_head_branch_name().unwrap_or_else(|_| "HEAD".to_string());

    match (&cmd.from, &cmd.to) {
        (Some(_), Some(_)) => Err("cannot specify both --from and --to".into()),
        (Some(remote_name), None) => {
            if cmd.force {
                return Err("--force only applies when copying to a remote".into());
            }
            let local_branch = cmd.as_branch.as_deref().unwrap_or(&head_branch);
            copy_from(space, remote_name, local_branch, cmd.branch.as_deref())
        }
        (None, to) => {
            let remote_name = to.as_deref().unwrap_or("origin");
            let target_branch = cmd.branch.as_deref().unwrap_or(&head_branch);
            copy_to(space, remote_name, target_branch, cmd.force)
        }
    }
}

fn describe(stats: &lib::transfer::TransferStats) -> String {
    format!(
        "{} commit(s), {} object(s), {}",
        stats.commits,
        stats.objects,
        crate::commit::fmt_bytes(stats.bytes as usize)
    )
}

/// Fetch `remote_branch` (default: `local_branch`) from a remote and
/// fast-forward `local_branch` to it, updating the working tree if that
/// branch is checked out.
fn copy_from(
    space: &mut Space,
    remote_name: &str,
    local_branch: &str,
    remote_branch: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let remote = space.get_remote_by_name(remote_name)?;
    let remote_branch = remote_branch.unwrap_or(local_branch);
    let remote_hash = remote
        .resolve_ref_name(ObjectReference::Ref(remote_branch.to_string()))
        .map_err(|_| format!("{} has no branch '{}'", remote_name, remote_branch))?;

    let is_current = space.get_ref("HEAD".into())? == ObjectReference::Ref(local_branch.to_string());
    if is_current && space.worktree.is_some() && crate::status::collect(space)?.is_dirty() {
        return Err("you have uncommitted changes; commit or discard them before copying from a remote".into());
    }

    let local_hash = space
        .resolve_ref_name(ObjectReference::Ref(local_branch.to_string()))
        .unwrap_or_else(|_| Hash::new());
    let stats = remote.copy_commit_to(space, remote_hash)?;

    if !space.is_ancestor(local_hash, remote_hash)? {
        return Err(format!(
            "'{}' has commits that {}/{} doesn't; duh can't merge, so only fast-forwards are copied",
            local_branch, remote_name, remote_branch
        )
        .into());
    }

    space.set_ref(
        local_branch,
        ObjectReference::Hash(remote_hash),
        Some(&format!("copy from {}/{}", remote_name, remote_branch)),
    )?;
    if is_current && space.worktree.is_some() {
        crate::checkout::checkout_tree(space, local_hash, remote_hash)?;
    }

    println!(
        "{} {}/{} -> {} ({}..{}): {}",
        crate::colors::cyan("Copied"),
        remote_name,
        remote_branch,
        crate::colors::green(local_branch),
        short(local_hash),
        short(remote_hash),
        describe(&stats)
    );
    Ok(())
}

/// Copy HEAD's history to a remote and point `target_branch` there.
fn copy_to(
    space: &mut Space,
    remote_name: &str,
    target_branch: &str,
    force: bool,
) -> Result<(), Box<dyn Error>> {
    let remote = space.get_remote_by_name(remote_name)?;
    let head = space.get_head_commit_hash()?;
    if head.is_zero() {
        return Err("nothing to copy: no commits yet".into());
    }

    // Like git, don't move the branch a non-bare remote has checked out:
    // its working files would no longer match.
    let remote_is_bare = remote.get_config_value("core.bare").map(|v| v == "true").unwrap_or(true);
    if !remote_is_bare && remote.get_head_branch_name().ok().as_deref() == Some(target_branch) {
        return Err(format!(
            "refusing to update '{}', the branch checked out in {}; copy to a bare space or another branch",
            target_branch, remote_name
        )
        .into());
    }

    let remote_hash = remote
        .resolve_ref_name(ObjectReference::Ref(target_branch.to_string()))
        .unwrap_or_else(|_| Hash::new());
    if !force && !space.is_ancestor(remote_hash, head)? {
        return Err(format!(
            "{}/{} has commits you don't; copy --from {} first (or --force to discard them)",
            remote_name, target_branch, remote_name
        )
        .into());
    }

    let stats = space.copy_commit_to(&remote, head)?;
    remote.set_ref(
        target_branch,
        ObjectReference::Hash(head),
        Some(&format!("copy to: {}", target_branch)),
    )?;

    println!(
        "{} {} -> {}/{} ({}..{}): {}",
        crate::colors::cyan("Copied"),
        space.get_head_branch_name().unwrap_or_else(|_| "HEAD".into()),
        remote_name,
        crate::colors::green(target_branch),
        short(remote_hash),
        short(head),
        describe(&stats)
    );
    Ok(())
}

fn short(h: Hash) -> String {
    if h.is_zero() {
        "(none)".to_string()
    } else {
        h.to_string()[..12].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::default_clone_dir;

    #[test]
    fn clone_dir_from_url() {
        for (url, want) in [
            ("/srv/repos/proj", "proj"),
            ("/srv/repos/proj/", "proj"),
            ("/srv/repos/proj/.duh", "proj"),
            ("/srv/repos/proj/.duh/", "proj"),
            ("file:///srv/repos/proj", "proj"),
            ("../proj", "proj"),
            ("s3://bucket/proj.duh", "proj"),
            ("proj", "proj"),
        ] {
            assert_eq!(default_clone_dir(url).unwrap(), want, "url {}", url);
        }
    }

    #[test]
    fn clone_dir_needs_a_name() {
        for url in ["/", ".duh", "./", "..", "file:///"] {
            assert!(default_clone_dir(url).is_err(), "url {}", url);
        }
    }
}
