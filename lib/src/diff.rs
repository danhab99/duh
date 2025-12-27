use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Result, Write};
use std::rc::Rc;
use std::{fmt, io};

use rmp::decode::RmpRead;
use rmp::encode::RmpWrite;

#[derive(PartialEq, Eq, Clone, Debug, serde::Deserialize, serde::Serialize)]
pub enum DiffFragment {
    ADDED { body: Vec<u8> },
    UNCHANGED { len: usize },
    DELETED { len: usize },
}

impl fmt::Display for DiffFragment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiffFragment::ADDED { body } => write!(f, "+{}", body.len()),
            DiffFragment::UNCHANGED { len } => write!(f, "={}", len),
            DiffFragment::DELETED { len } => write!(f, "-{}", len),
        }
    }
}

pub fn collect_until_convergence(old: &[u8], new: &[u8]) -> (Vec<DiffFragment>, usize, usize) {
    const MASK: u64 = 0b1111_1111;
    const BASE: u64 = 131;

    let mut old_index: HashMap<u64, usize> = HashMap::new();

    let mut hash = 0u64;
    let mut len = 0usize;
    let mut old_pos = 0usize;

    for &b in old {
        hash = hash.wrapping_mul(BASE).wrapping_add(b as u64);
        len += 1;
        old_pos += 1;

        if hash & MASK == 0 {
            old_index.insert(hash, len);
            hash = 0;
            len = 0;
        }
    }

    if len > 0 {
        old_index.insert(hash, len);
    }

    let mut diffs = Vec::new();

    let mut hash = 0u64;
    let mut chunk: Vec<u8> = Vec::new();
    let mut new_pos = 0usize;

    for &b in new {
        hash = hash.wrapping_mul(BASE).wrapping_add(b as u64);
        chunk.push(b);
        new_pos += 1;

        if hash & MASK == 0 {
            if let Some(old_len) = old_index.get(&hash) {
                // convergence found
                if !chunk.is_empty() {
                    diffs.push(DiffFragment::ADDED {
                        body: chunk.clone(),
                    });
                }

                diffs.push(DiffFragment::DELETED { len: *old_len });

                return (diffs, old_pos - *old_len, new_pos);
            } else {
                diffs.push(DiffFragment::ADDED {
                    body: std::mem::take(&mut chunk),
                });
                hash = 0;
            }
        }
    }

    if !chunk.is_empty() {
        diffs.push(DiffFragment::ADDED { body: chunk });
    }

    diffs.push(DiffFragment::DELETED { len: old.len() });

    (diffs, old.len(), new.len())
}

fn roll_convergence<R: Read>(mut old: R, mut new: R) -> Result<(usize, DiffFragment)> {
    let mut old_hasher = blake3::Hasher::new();
    let mut new_hasher = blake3::Hasher::new();
    let mut unchanged_len = 0usize;

    loop {
        let _ = old_hasher.write_u8(old.read_u8()?);
        let _ = new_hasher.write_u8(new.read_u8()?);

        let old_hash = old_hasher.finalize();
        let new_hash = new_hasher.finalize();

        unchanged_len += 1;

        if old_hash != new_hash {
            break;
        }
    }

    Ok((
        unchanged_len,
        DiffFragment::UNCHANGED { len: unchanged_len },
    ))
}

pub fn build_diff_fragments<R: Read>(
    old: R,
    new: R,
    block_size: usize,
) -> io::Result<Vec<DiffFragment>> {
    type Buffer = Vec<u8>;

    let read_buffer = |reader: &mut R| -> io::Result<Buffer> {
        let mut buf = vec![0u8; block_size];
        let n = reader.read(&mut buf)?;
        buf.truncate(n);
        Ok(buf)
    };

    let fragments = Rc::new(RefCell::new(Vec::new()));

    let old_rc = Rc::new(RefCell::new(old));
    let new_rc = Rc::new(RefCell::new(new));

    let collect_unchanged = {
        let fragments = Rc::clone(&fragments);
        let old_clone = Rc::clone(&old_rc);
        let new_clone = Rc::clone(&new_rc);
        move || -> Result<usize> {
            let mut old_ref = old_clone.borrow_mut();
            let mut new_ref = new_clone.borrow_mut();
            let (size, frag) = roll_convergence(&mut *old_ref, &mut *new_ref)?;
            fragments.borrow_mut().push(frag);
            Ok(size)
        }
    };

    let collected_changed = {
        let fragments = Rc::clone(&fragments);
        let old_clone = Rc::clone(&old_rc);
        let new_clone = Rc::clone(&new_rc);
        move || -> io::Result<usize> {
            let mut old_ref = old_clone.borrow_mut();
            let mut new_ref = new_clone.borrow_mut();
            let old_buffer = read_buffer(&mut *old_ref)?;
            let new_buffer = read_buffer(&mut *new_ref)?;
            let (mut frags, _, _) =
                collect_until_convergence(old_buffer.as_slice(), new_buffer.as_slice());

            let length = frags.iter().fold(0usize, |sum, x| {
                sum + match x {
                    DiffFragment::ADDED { body } => body.len(),
                    DiffFragment::UNCHANGED { len } | DiffFragment::DELETED { len } => *len,
                }
            });

            fragments.borrow_mut().append(&mut frags);

            return Ok(length);
        }
    };

    let mut changing = false;
    let mut last_len = 1;

    while last_len > 0 {
        last_len = if changing {
            collect_unchanged()?
        } else {
            collected_changed()?
        };

        if last_len == 0 {
            changing = !changing;
        }
    }

    Ok(Rc::try_unwrap(fragments)
        .expect("Multiple references to fragments")
        .into_inner())
}
