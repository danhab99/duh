use std::error::Error;

use clap::clap_derive::Args;
use lib::{objects::ObjectReference, space::Space};

#[derive(Args)]
#[command(about = "Stage a file (produce fragment objects without committing)")]
pub struct BranchCommand {
    #[arg(short = 'd', long = "delete", help = "Delete a branches")]
    delete: Option<String>,

    #[arg(short = 'r', long = "rename", help = "Rename a branches")]
    rename: Option<String>,

    #[arg(
        short = 's',
        long = "set",
        help = "Set the commit this branch points to"
    )]
    set: Option<String>,
}

pub fn branch<F: vfs::FileSystem>(space: &mut Space<F>, cmd: &BranchCommand) -> Result<(), Box<dyn Error>> {
    let head_ref = space.get_ref("HEAD".into())?;

    if let Some(name) = cmd.delete.clone() {
        let deleting_ref = space.get_ref(name.clone())?;

        if head_ref.eq(&deleting_ref) {
            println!("Unable to delete the branch you are on");
            return Err("detatched head".into());
        }

        println!("Deleting branch {}", name.clone());
        space.delete_ref(name.as_str())?;
    } else if let Some(name) = cmd.rename.clone() {
        let current_commit = space.resolve_ref_name(head_ref.clone())?;

        space.delete_ref(head_ref.to_string().as_str())?;
        space.set_ref(&name, ObjectReference::Hash(current_commit))?;
        // Update HEAD to point to the new branch name; the old ref was just deleted.
        space.set_ref("HEAD", ObjectReference::Ref(name.clone()))?;
    } else if let Some(set) = cmd.set.clone() {
        let commit = ObjectReference::from(set);
        space.set_ref(head_ref.to_string().as_str(), commit)?;
    } else {
        println!("Listing all branches");

        let refs = space.list_refs("branch")?;

        for r in refs {
            if let ObjectReference::Ref(name) = r {
                println!("  {}", name);
            }
        }
    }

    Ok(())
}
