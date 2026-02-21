use std::error::Error;

use clap::clap_derive::Args;
use lib::hash::Hash;
use lib::objects::ObjectReference;
use lib::repo::Repo;

use crate::checkout::checkout;
use crate::status::status;

#[derive(Args)]
#[command(about = "switch a file (produce fragment objects without committing)")]
pub struct SwitchCommand {
    /// Path to the file to switch
    #[arg(help = "Name of branch to switch to")]
    pub name: String,

    pub create: Option<bool>,
}

pub fn switch(repo: &mut Repo, cmd: &SwitchCommand) -> Result<(), Box<dyn Error>> {
    let uncomitted_changes = status(repo, &crate::status::StatusCommand {})?;

    if uncomitted_changes && cmd.create == None || cmd.create == Some(false) {
        panic!("uncommitted changes");
    }

    println!("{} {}", crate::colors::cyan("Staging file"), cmd.name);
    // let h = repo.switch_file(cmd.name.clone())?;

    let branch_name = cmd.name.clone();

    let ref_exists = match repo.get_ref(branch_name.clone()) {
        Err(_) => false,
        Ok(ObjectReference::Ref(_)) => true,
        Ok(ObjectReference::Hash(_)) => panic!("cannot switch to commit rn"),
    };

    let commit_hash: Hash;

    if ref_exists {
        repo.set_ref("HEAD", ObjectReference::Ref(branch_name.clone()))?;
        commit_hash = repo.resolve_ref_name(ObjectReference::Ref(branch_name.clone()))?;
    } else if cmd.create == Some(true) {
        commit_hash = match repo.get_ref("HEAD".into())? {
            ObjectReference::Hash(h) => h,
            _ => panic!("why is this branch pointed at another branch?"),
        };

        repo.set_ref(&cmd.name, ObjectReference::Hash(commit_hash.clone()))?;
        repo.set_ref("HEAD", ObjectReference::Ref(branch_name.clone()))?;
    } else {
        panic!("ref does not exist");
    }

    let files = repo.list_files(ObjectReference::Hash(commit_hash))?;

    for file_path in files {
        checkout(
            repo,
            &crate::checkout::CheckoutCommand {
                file_path,
                commit: Some(commit_hash.to_string()),
            },
        );
    }

    Ok(())
}
