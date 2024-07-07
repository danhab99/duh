use lib::diff;
use crate::repo::Repo;

pub fn diff(old: &String, new: &String) {

    let repo = Repo::at_root_path(None);
    let old_path = repo.get_path_in_cwd(old);
    let new_path = repo.get_path_in_cwd(new);

    let old_content = std::fs::read(old_path).unwrap();
    let new_content = std::fs::read(new_path).unwrap();

    let diffs = diff::diff_content(&old_content, &new_content);

    for diff in diffs {
        match diff {
            diff::DiffFragment::ADDED { offset, body } => {
                println!("Added offset={} data={:02X?}", offset, body)
            }
            diff::DiffFragment::UNCHANGED { offset, len } => {
                println!("Nothing changed from {} to {}", offset, len)
            }
            diff::DiffFragment::DELETED { offset, len } => {
                println!("Deleted offset={} len={}", offset, len)
            }
        }
    }
}
