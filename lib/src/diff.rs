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
