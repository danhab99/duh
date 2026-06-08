use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::{Object, ObjectReference};
use lib::space::Space;

use crate::checkout::checkout;

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

#[derive(Args)]
#[command(about = "Reset the current branch to a specified commit")]
pub struct ResetCommand {
    /// Reset mode
    #[arg(short = 'm', long = "mode", default_value = "mixed")]
    pub mode: ResetMode,

    /// Commit to reset to. Defaults to HEAD.
    #[arg(help = "Commit hash or ref to reset to (defaults to HEAD)")]
    pub commit: Option<ObjectReference>,
}

pub fn reset<F: vfs::FileSystem>(space: &mut Space<F>, cmd: &ResetCommand) -> Result<(), Box<dyn Error>> {
    let target_hash = match &cmd.commit {
        Some(r) => space.resolve_ref_name(r.clone())?,
        None => space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?,
    };

    match space.get_object(target_hash)? {
        Some(Object::Commit(c)) => {
            match cmd.mode {
                ResetMode::Soft | ResetMode::Mixed => {
                    space.index = c.files;
                    println!(
                        "{} index to {}",
                        crate::colors::green("Reset"),
                        crate::colors::cyan(&target_hash.to_string())
                    );
                }
                ResetMode::Hard => {
                    space.index = c.files;

                    let files = space.list_files(ObjectReference::Hash(target_hash))?;

                    for file_path in files {
                        checkout(
                            space,
                            &crate::checkout::CheckoutCommand {
                                file_path,
                                commit: Some(ObjectReference::Hash(target_hash)),
                                as_name: None,
                            },
                        )?;
                    }

                    println!(
                        "{} working tree and index to {}",
                        crate::colors::green("Reset"),
                        crate::colors::cyan(&target_hash.to_string())
                    );
                }
            }
        }
        _ => {
            return Err(format!("{:?} is not a commit", target_hash).into());
        }
    }

    Ok(())
}
