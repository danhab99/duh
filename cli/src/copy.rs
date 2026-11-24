use std::error::Error;
use std::path::PathBuf;

use clap::clap_derive::Args;
use lib::{hash::Hash, objects::ObjectReference, space::Space};
use opendal::services::Fs;

use crate::checkout::restore;

#[derive(Args)]
#[command(about = "Copy commits between local and remote repositories, or clone a remote")]
pub struct CopyCommand {
    #[arg(
        short = 'f',
        long = "from",
        help = "Remote name to copy from (defaults to 'origin'), or a URL to clone from when outside a duh space"
    )]
    pub from: Option<String>,

    #[arg(
        short = 't',
        long = "to",
        help = "Remote name to push to (defaults to 'origin')"
    )]
    pub to: Option<String>,

    #[arg(
        short = 'a',
        long = "as",
        help = "Local branch name to receive commits (for --from), or destination directory name when cloning"
    )]
    pub as_branch: Option<String>,

    #[arg(
        short = 'b',
        long = "branch",
        help = "Remote branch name (defaults to current HEAD branch)"
    )]
    pub branch: Option<String>,

    #[arg(long = "bare", help = "Clone/copy without checking out a working tree")]
    pub bare: bool,
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
    clone_into(url, &dest_path, &metadata_path, remote_branch, bare).map_err(|e| {
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
) -> Result<(), Box<dyn Error>> {
    // Initialize a new duh space
    let op = Fs::default().root(metadata_path.to_str().unwrap());
    let fs = opendal::Operator::new(op)?.finish().blocking();
    let worktree = if bare { None } else { Some(dest_path.clone()) };
    let mut space = Space::initialize_at(fs, worktree)?;

    // Add the remote as "origin"
    space.add_remote("origin", url)?;

    // Open the remote
    let mut remote = space.get_remote_by_name("origin")?;

    // Determine which branch to clone
    let head_name = remote
        .get_head_branch_name()
        .unwrap_or_else(|_| "HEAD".to_string());
    let target_branch = remote_branch.unwrap_or(&head_name);

    // Resolve the remote branch hash
    let remote_hash = remote
        .resolve_ref_name(ObjectReference::Ref(target_branch.to_string()))
        .unwrap_or_else(|_| remote.get_head_commit_hash().unwrap_or_else(|_| Hash::new()));

    // Copy commits from remote
    lib::remote::copy_commits(
        &mut remote,
        &mut space,
        remote_hash,
        None::<fn(lib::remote::CopyCommitsProgress)>,
    )?;

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

pub fn copy(
    space: &mut Space,
    cmd: &CopyCommand,
) -> Result<(), Box<dyn Error>> {
    let head_branch = space.get_head_branch_name().unwrap_or_else(|_| "HEAD".to_string());

    match (&cmd.from, &cmd.to) {
        (Some(remote_name), None) => {
            let local_branch = cmd.as_branch.as_deref().unwrap_or(&head_branch);
            copy_from(
                space,
                remote_name.as_str(),
                local_branch,
                cmd.branch.as_deref(),
                cmd.bare,
            )?
        }
        (None, Some(remote_name)) => {
            let target_branch = cmd.branch.as_deref().unwrap_or(&head_branch);
            copy_to(space, remote_name.as_str(), target_branch)?
        }
        (Some(_), Some(_)) => {
            return Err("cannot specify both --from and --to".into());
        }
        (None, None) => {
            let target_branch = cmd.branch.as_deref().unwrap_or(&head_branch);
            copy_to(space, "origin", target_branch)?
        }
    }

    Ok(())
}

fn copy_from(
    space: &mut Space,
    remote_name: &str,
    local_branch: &str,
    remote_branch: Option<&str>,
    bare: bool,
) -> Result<(), Box<dyn Error>> {
    let mut remote = space.get_remote_by_name(remote_name)?;

    let target_remote_branch = remote_branch.unwrap_or(local_branch);

    let remote_hash = remote
        .resolve_ref_name(ObjectReference::Ref(target_remote_branch.to_string()))
        .unwrap_or_else(|_| remote.get_head_commit_hash().unwrap_or_else(|_| Hash::new()));

    lib::remote::copy_commits(
        &mut remote,
        space,
        remote_hash,
        None::<fn(lib::remote::CopyCommitsProgress)>,
    )?;

    space.set_ref(
        local_branch,
        ObjectReference::Hash(remote_hash),
        Some(&format!("copy from {}: {}", remote_name, local_branch)),
    )?;

    space.set_ref(
        "HEAD",
        ObjectReference::Ref(local_branch.to_string()),
        Some("copy from remote"),
    )?;

    if !bare {
        let files = space.list_files(ObjectReference::Hash(remote_hash))?;

        for file_path in files {
            restore(space, &file_path, remote_hash, None)?;
        }
    }

    println!(
        "{} {} from {} -> {}",
        crate::colors::cyan("Copied"),
        remote_hash.to_hex(),
        remote_name,
        crate::colors::green(local_branch)
    );

    Ok(())
}

fn copy_to(
    space: &mut Space,
    remote_name: &str,
    target_branch: &str,
) -> Result<(), Box<dyn Error>> {
    let mut remote = space.get_remote_by_name(remote_name)?;

    let head_hash = space.get_head_commit_hash()?;

    lib::remote::copy_commits(space, &mut remote, head_hash, Some(|_| {}))?;

    remote.set_ref(
        target_branch,
        ObjectReference::Hash(head_hash),
        Some(&format!("push: {}", target_branch)),
    )?;

    println!(
        "{} {} to {} -> {}",
        crate::colors::cyan("Pushed"),
        crate::colors::yellow(&head_hash.to_hex()),
        crate::colors::yellow(remote_name),
        crate::colors::green(target_branch)
    );

    Ok(())
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
