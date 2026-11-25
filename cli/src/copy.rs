use std::error::Error;
use std::path::PathBuf;

use clap::clap_derive::Args;
use lib::ops::fmt_bytes;
use lib::{hash::Hash, space::Space};

#[derive(Args)]
#[command(about = "Send your saves to a place (fast-forward only)")]
pub struct SendCommand {
    /// Place to send to (default: origin)
    pub place: Option<String>,

    /// Draft to update there (default: the one you're on)
    #[arg(short = 'd', long = "draft", alias = "branch", short_alias = 'b')]
    pub draft: Option<String>,

    /// Overwrite the place's draft even if that discards saves made there
    #[arg(long = "force")]
    pub force: bool,
}

#[derive(Args)]
#[command(about = "Receive new saves from a place (fast-forward only)")]
pub struct ReceiveCommand {
    /// Place to receive from (default: origin)
    pub place: Option<String>,

    /// Draft to receive from there (default: the same name as yours)
    #[arg(short = 'd', long = "draft", alias = "branch", short_alias = 'b')]
    pub draft: Option<String>,

    /// Your draft to update (default: the one you're on)
    #[arg(long = "into")]
    pub into: Option<String>,
}

#[derive(clap::Args)]
#[command(about = "Get a copy of a space into a new folder")]
pub struct CloneCommand {
    /// Where the space lives: a folder, file:// URL or storage URL (s3://…)
    pub url: String,

    /// Folder to create (default: named after the URL)
    pub directory: Option<String>,

    /// Draft to open (default: the one the space is on)
    #[arg(short = 'd', long = "draft", alias = "branch", short_alias = 'b')]
    pub branch: Option<String>,

    /// Only the history, no working files
    #[arg(long = "bare")]
    pub bare: bool,

    /// Storage option or credential, saved in the new space's settings
    /// (e.g. -o region=eu-west-1 -o access_key_id=…)
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
            "can't infer a directory name from '{}'; name the folder: duh get <url> <folder>",
            url
        )
        .into());
    }
    Ok(name.to_string())
}

/// Get a copy of a space into a new folder.
pub fn clone(
    url: &str,
    dest_dir: &str,
    remote_branch: Option<&str>,
    bare: bool,
    options: &[(String, String)],
) -> Result<(), Box<dyn Error>> {
    lib::ops::sharing::get(url, &PathBuf::from(dest_dir), remote_branch, bare, options)?;
    println!("{} {} -> {}", crate::colors::cyan("Got"), crate::colors::green(url), crate::colors::green(dest_dir));
    Ok(())
}

fn describe(stats: &lib::transfer::TransferStats) -> String {
    format!("{} save(s), {} object(s), {}", stats.commits, stats.objects, fmt_bytes(stats.bytes))
}

fn short(h: Hash) -> String {
    if h.is_zero() { "(none)".to_string() } else { h.to_string()[..12].to_string() }
}

pub fn send(space: &mut Space, cmd: &SendCommand) -> Result<(), Box<dyn Error>> {
    let place = cmd.place.as_deref().unwrap_or("origin");
    let t = lib::ops::sharing::send(space, place, cmd.draft.as_deref(), cmd.force)?;
    println!(
        "{} {} -> {}/{} ({}..{}): {}",
        crate::colors::cyan("Sent"),
        t.source_draft,
        t.place,
        crate::colors::green(&t.draft),
        short(t.before),
        short(t.after),
        describe(&t.stats)
    );
    Ok(())
}

pub fn receive(space: &mut Space, cmd: &ReceiveCommand) -> Result<(), Box<dyn Error>> {
    let place = cmd.place.as_deref().unwrap_or("origin");
    let t = lib::ops::sharing::receive(space, place, cmd.draft.as_deref(), cmd.into.as_deref())?;
    println!(
        "{} {}/{} -> {} ({}..{}): {}",
        crate::colors::cyan("Received"),
        t.place,
        t.source_draft,
        crate::colors::green(&t.draft),
        short(t.before),
        short(t.after),
        describe(&t.stats)
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
