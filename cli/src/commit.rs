use lib::{objects::ObjectReference, repo::Repo};
use serde::de::Error;

use crate::cli::CommitCommand;

pub fn commit(repo: Repo, cmd: CommitCommand) -> Result<(), Box<dyn Error>> {
    let head_ref = repo.get_ref("HEAD".into())?;
    let head_commit_hash = repo.resolve_ref_name(head_ref)?;

    let commit_hash = repo.commit(cmd.message, &cmd.start, Some(head_commit_hash))?;

    repo.set_ref(
        match head_ref {
            ObjectReference::Hash(_) => "HEAD",
            ObjectReference::Ref(ref_name) => &ref_name,
        },
        commit_hash,
    );

    Ok(())
}
