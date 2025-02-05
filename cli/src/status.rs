use lib::repo::Repo;
use clap::clap_derive::Args;

#[derive(Args)]
pub struct StatusCommand {
    #[arg(short, long)]
    pub wd: Option<String>,
}

pub fn status(repo: Repo, cmd: &StatusCommand) {
    let files = std::fs::read_dir(repo.get_path_in_cwd(""))
        .unwrap()
        .map(|x| x.unwrap())
        .collect::<Vec<_>>();

    println!("STATUS {:?}", files);
}
