use lib::diff;

pub fn diff(old: &String, new: &String) {
    let old_content = std::fs::read(old).unwrap();
    let new_content = std::fs::read(new).unwrap();

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
