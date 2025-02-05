use std::borrow::Borrow;

use clap::clap_derive::Args;
use lib::{objects::ObjectReference, repo::Repo};

#[derive(Args)]
pub struct CommitCommand {
    pub start: String,
    pub message: String,
}

pub fn commit(repo: Repo, cmd: &CommitCommand) {
    let head_ref = repo.get_ref("HEAD".into()).unwrap();
    let head_commit_hash = repo.resolve_ref_name(head_ref).unwrap();

    let commit_hash = repo
        .commit(cmd.message.clone(), &cmd.start, Some(head_commit_hash))
        .unwrap();

    match head_ref {
        ObjectReference::Hash(hash) => panic 
    }

    repo.set_ref(
        match head_ref {
        },
        ObjectReference::Hash(commit_hash),
    )
    .unwrap();
}
