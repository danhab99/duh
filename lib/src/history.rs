use crate::diff::{self, DiffFragment};

pub struct FileHistory {
    diffs: Vec<Vec<DiffFragment>>,
}

impl FileHistory {
    pub fn new() -> FileHistory {
        FileHistory { diffs: Vec::new() }
    }

    pub fn rebuild_file(&self, limit: Option<usize>) -> &[u8] {
        let mut content = Vec::<u8>::new();

        for diffs in self.diffs[0..limit.unwrap_or(self.diffs.len())].iter() {
            for diff in diffs {
                match diff {
                    DiffFragment::ADDED { offset, body } => {
                        let end_pos = offset + body.len();
                        content = content.splice(*offset..end_pos, *body).collect();
                    }
                    DiffFragment::UNCHANGED { offset, len } => {}
                    DiffFragment::DELETED { offset, len } => {
                        let end_pos = offset + len;
                        content = content.splice(*offset..end_pos, Vec::new()).collect();
                    }
                }
            }
        }

        &content.as_slice()
    }

    pub fn superimpose_content(&mut self, new: &[u8]) {
        let old = self.rebuild_file(None);
        let diff = diff::diff_content(old, new);
        self.diffs.push(diff);
    }
}
