use lib::repo::Repo;
use lib::diff::{ diff_content, DiffFragment };
use clap::clap_derive::Args;

#[derive(Args)]
pub struct DiffCommand {
    pub old: String,
    pub new: String,
}

pub fn diff(repo: Repo, cmd: &DiffCommand) {
    let old_path = repo.get_path_in_cwd(cmd.old.as_str());
    let new_path = repo.get_path_in_cwd(cmd.new.as_str());

    let old_content = std::fs::read(old_path).unwrap();
    let new_content = std::fs::read(new_path).unwrap();

    let diffs = diff_content(&old_content, &new_content);

    for diff in diffs {
        match diff {
            DiffFragment::ADDED { offset, body } => {
                println!("Added offset={} data={:02X?}", offset, body)
            }
            DiffFragment::UNCHANGED { offset, len } => {
                println!("Nothing changed from {} to {}", offset, len)
            }
            DiffFragment::DELETED { offset, len } => {
                println!("Deleted offset={} len={}", offset, len)
            }
        }
    }
}
