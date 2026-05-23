use std::error::Error;

use clap::clap_derive::Args;
use lib::hash::Hash;
use lib::objects::ObjectReference;
use lib::space::Space;

use crate::checkout::checkout;
use crate::status::status;

#[derive(Args)]
#[command(about = "switch a file (produce fragment objects without committing)")]
pub struct SwitchCommand {
    /// Path to the file to switch
    #[arg(help = "Name of branch to switch to")]
    pub name: String,

    #[arg(help = "Create a new branch")]
    pub create: Option<bool>,
}

pub fn switch<F: vfs::FileSystem>(space: &mut Space<F>, cmd: &SwitchCommand) -> Result<(), Box<dyn Error>> {
    let uncomitted_changes = status(space, &crate::status::StatusCommand {})?;

    if uncomitted_changes && cmd.create == None || cmd.create == Some(false) {
        return Err(Box::new(lib::error::DuhError::UncommittedChanges));
    }

    println!("{} {}", crate::colors::cyan("Switching to"), cmd.name);

    let branch_name = cmd.name.clone();

    // Just check existence — all branch refs contain a commit hash, so matching on
    // Hash(_) and panicking was wrong.
    let ref_exists = space.get_ref(branch_name.clone()).is_ok();

    let commit_hash: Hash;

    if ref_exists {
        space.set_ref("HEAD", ObjectReference::Ref(branch_name.clone()))?;
        commit_hash = space.resolve_ref_name(ObjectReference::Ref(branch_name.clone()))?;
    } else if cmd.create == Some(true) {
        // resolve_ref_name handles both attached HEAD (Ref -> Hash) and detached HEAD (Hash directly).
        commit_hash = space.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;
        space.set_ref(&cmd.name, ObjectReference::Hash(commit_hash.clone()))?;
        space.set_ref("HEAD", ObjectReference::Ref(branch_name.clone()))?;
    } else {
        return Err(format!("branch '{}' does not exist", branch_name).into());
    }

    let files = space.list_files(ObjectReference::Hash(commit_hash))?;

    for file_path in files {
        checkout(
            space,
            &crate::checkout::CheckoutCommand {
                file_path,
                commit: Some(ObjectReference::Hash(commit_hash)),
            },
        )?;
    }

    Ok(())
}
