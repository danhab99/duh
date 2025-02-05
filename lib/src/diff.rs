use diff::Result as DiffResult;
use serde::{Deserialize, Serialize};
use std::borrow::BorrowMut;

#[derive(PartialEq, Eq, Clone, Debug, Deserialize, Serialize)]
pub enum DiffFragment {
    ADDED { offset: usize, body: Vec<u8> },
    UNCHANGED { offset: usize, len: usize },
    DELETED { offset: usize, len: usize },
}

pub fn diff_content(old: &[u8], new: &[u8]) -> Vec<DiffFragment> {
    let min_len = old.len().min(new.len());

    let delta = diff::slice(&old[0..min_len], &new[0..min_len]);

    let mut squished_frags = Vec::<DiffFragment>::new();

    for (i, d) in delta.iter().enumerate() {
        let mut l = squished_frags.last_mut();

        match (l.borrow_mut(), d) {
            (Some(DiffFragment::ADDED { offset: _, body }), DiffResult::Right(d)) => {
                body.push(**d);
            }
            (Some(DiffFragment::UNCHANGED { offset: _, len }), DiffResult::Both(_, _)) => {
                *len += 1;
            }
            (Some(DiffFragment::DELETED { offset: _, len }), DiffResult::Left(_)) => {
                *len += 1;
            }
            (_, DiffResult::Left(_)) => squished_frags.push(DiffFragment::DELETED {
                offset: i as usize,
                len: 1,
            }),
            (_, DiffResult::Both(_, _)) => squished_frags.push(DiffFragment::UNCHANGED {
                offset: i as usize,
                len: 1,
            }),
            (_, DiffResult::Right(b)) => squished_frags.push(DiffFragment::ADDED {
                offset: i as usize,
                body: vec![**b],
            }),
        }
    }

    if old.len() > new.len() {
        squished_frags.push(DiffFragment::DELETED {
            offset: new.len() as usize,
            len: (old.len() - new.len()) as usize,
        })
    } else if old.len() < new.len() {
        let old_len = old.len();
        let new_len = new.len();
        squished_frags.push(DiffFragment::ADDED {
            offset: old_len as usize,
            body: new[old_len..new_len].to_vec(),
        })
    }

    squished_frags
}

pub fn best_alignment(left: &[u8], right: &[u8]) -> isize {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut best_match = 0;
    let mut best_offset = 0;

    // Compute a rolling hash function
    fn rolling_hash(slice: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        slice.hash(&mut hasher);
        hasher.finish()
    }

    // Iterate over possible alignments
    for offset in -(left.len() as isize)..=(right.len() as isize) {
        let start_left = if offset < 0 { (-offset) as usize } else { 0 };
        let start_right = if offset > 0 { offset as usize } else { 0 };
        
        let overlap_len = (left.len() - start_left).min(right.len() - start_right);
        if overlap_len == 0 {
            continue;
        }

        let mut match_len = 0;
        let left_sub = &left[start_left..start_left + overlap_len];
        let right_sub = &right[start_right..start_right + overlap_len];

        // Compare hashes first, then verify if they match
        if rolling_hash(left_sub) == rolling_hash(right_sub) {
            match_len = overlap_len;  // Full match
        } else {
            // Fallback to byte-by-byte if hash collision occurs
            for i in 0..overlap_len {
                if left_sub[i] != right_sub[i] {
                    break;
                }
                match_len += 1;
            }
        }

        if match_len > best_match {
            best_match = match_len;
            best_offset = offset;
        }
    }

    best_offset
}
