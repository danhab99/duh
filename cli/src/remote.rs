use std::error::Error;

use clap::{clap_derive::Args, Subcommand};
use lib::space::Space;

#[derive(Args)]
#[command(about = "Manage places your space is shared to (folders, S3, Drive, …)")]
pub struct RemoteCommand {
    #[command(subcommand)]
    pub action: RemoteAction,
}

#[derive(Subcommand)]
pub enum RemoteAction {
    /// List places
    #[command(name = "list", visible_alias = "ls")]
    List {
        /// Show URLs alongside names
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
    },
    /// Add a place: a folder, file:// URL, or storage URL
    /// (s3://bucket/path, gcs://bucket/path, webdav://host/path, …)
    Add {
        /// Name for the place, e.g. origin or studio-drive
        name: String,
        /// Where it is
        url: String,
        /// Storage option or credential, stored in plain text in the settings
        /// (e.g. -o region=eu-west-1 -o access_key_id=… -o secret_access_key=…)
        #[arg(short = 'o', long = "option", value_name = "KEY=VALUE")]
        options: Vec<String>,
    },
    /// Remove a place (nothing stored there is deleted)
    #[command(name = "remove", visible_alias = "rm")]
    Remove {
        /// Place to remove
        name: String,
    },
    /// Rename a place
    Rename {
        /// Current name
        old_name: String,
        /// New name
        new_name: String,
    },
    /// Show where a place is
    #[command(name = "get-url")]
    GetUrl {
        /// Place name
        name: String,
    },
    /// Change where a place is
    #[command(name = "set-url")]
    SetUrl {
        /// Place name
        name: String,
        /// New URL
        url: String,
    },
}

pub fn remote(
    space: &mut Space,
    cmd: &RemoteCommand,
) -> Result<(), Box<dyn Error>> {
    match &cmd.action {
        RemoteAction::List { verbose } => {
            let remotes = space.list_remotes()?;
            if remotes.is_empty() {
                println!("No places yet (`duh places add <name> <url>`).");
                return Ok(());
            }

            if *verbose {
                for (name, url) in remotes {
                    println!(
                        "{}\t{} (receive)",
                        crate::colors::cyan(&name),
                        url
                    );
                    println!(
                        "{}\t{} (send)",
                        crate::colors::cyan(&name),
                        url
                    );
                }
            } else {
                for (name, _) in remotes {
                    println!("{}", crate::colors::cyan(&name));
                }
            }
        }
        RemoteAction::Add { name, url, options } => {
            let options = options
                .iter()
                .map(|o| lib::store::parse_option(o))
                .collect::<Result<Vec<_>, _>>()?;
            space.add_remote_with_options(name, url, &options)?;
            println!(
                "{} the place {} at {}. `duh send {}` copies your saves there.",
                crate::colors::green("Added"),
                crate::colors::cyan(name),
                url,
                name
            );
        }
        RemoteAction::Remove { name } => {
            space.remove_remote(name)?;
            println!(
                "{} the place {} (nothing stored there was deleted)",
                crate::colors::green("Removed"),
                crate::colors::cyan(name)
            );
        }
        RemoteAction::Rename { old_name, new_name } => {
            space.rename_remote(old_name, new_name)?;
            println!(
                "{} the place {} to {}",
                crate::colors::green("Renamed"),
                crate::colors::cyan(old_name),
                crate::colors::cyan(new_name)
            );
        }
        RemoteAction::GetUrl { name } => {
            let url = space.get_remote_url(name)?;
            println!("{}", url);
        }
        RemoteAction::SetUrl { name, url } => {
            lib::store::validate_url(url)?;
            space.set_remote_url(name, &lib::store::normalize_url(url)?)?;
            println!(
                "{} place '{}' -> {}",
                crate::colors::green("set url"),
                crate::colors::cyan(name),
                url
            );
        }
    }

    Ok(())
}
