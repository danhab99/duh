use lib::repo::Repo;
use clap::clap_derive::Args;

#[derive(Args)]
pub struct TrackCommand {
    pub names: Vec<String>,
}

pub fn track(repo: Repo, cmd: &TrackCommand) {
    for n in cmd.names {
        repo.stage_file(n.as_str()).unwrap();
    }
}
