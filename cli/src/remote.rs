use std::error::Error;

use clap::{clap_derive::Args, Subcommand};
use lib::space::Space;

#[derive(Args)]
#[command(about = "Manage remote repository connections")]
pub struct RemoteCommand {
    #[command(subcommand)]
    pub action: RemoteAction,
}

#[derive(Subcommand)]
pub enum RemoteAction {
    /// List configured remotes
    #[command(name = "list", visible_alias = "ls")]
    List {
        /// Show URLs alongside names
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
    },
    /// Add a new remote
    Add {
        /// Remote name
        name: String,
        /// Remote URL
        url: String,
    },
    /// Remove a remote
    #[command(name = "remove", visible_alias = "rm")]
    Remove {
        /// Remote name to remove
        name: String,
    },
    /// Rename a remote
    Rename {
        /// Old remote name
        old_name: String,
        /// New remote name
        new_name: String,
    },
    /// Get the URL for a remote
    #[command(name = "get-url")]
    GetUrl {
        /// Remote name
        name: String,
    },
    /// Set the URL for a remote
    #[command(name = "set-url")]
    SetUrl {
        /// Remote name
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
                println!("No remotes configured.");
                return Ok(());
            }

            if *verbose {
                for (name, url) in remotes {
                    println!(
                        "{}\t{} (fetch)",
                        crate::colors::cyan(&name),
                        url
                    );
                    println!(
                        "{}\t{} (push)",
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
        RemoteAction::Add { name, url } => {
            space.add_remote(name, url)?;
            println!(
                "{} remote '{}' -> {}",
                crate::colors::green("added"),
                crate::colors::cyan(name),
                url
            );
        }
        RemoteAction::Remove { name } => {
            space.remove_remote(name)?;
            println!(
                "{} remote '{}'",
                crate::colors::green("removed"),
                crate::colors::cyan(name)
            );
        }
        RemoteAction::Rename { old_name, new_name } => {
            space.rename_remote(old_name, new_name)?;
            println!(
                "{} remote '{}' -> '{}'",
                crate::colors::green("renamed"),
                crate::colors::cyan(old_name),
                crate::colors::cyan(new_name)
            );
        }
        RemoteAction::GetUrl { name } => {
            let url = space.get_remote_url(name)?;
            println!("{}", url);
        }
        RemoteAction::SetUrl { name, url } => {
            space.set_remote_url(name, url)?;
            println!(
                "{} remote '{}' -> {}",
                crate::colors::green("set url"),
                crate::colors::cyan(name),
                url
            );
        }
    }

    Ok(())
}
