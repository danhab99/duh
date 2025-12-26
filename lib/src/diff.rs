use std::collections::{HashMap, VecDeque};

use std::error::Error;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::ops::Index;
use std::{any, fmt, io, iter};

use crate::hash::Hash;

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

pub fn build_diff_fragments<R: Read>(
    old: R,
    new: R,
    block_size: usize,
) -> Result<(), Box<dyn Error>> {
    type Buffer = Vec<u8>;

    let new_buffer = || Vec::with_capacity(block_size);

    let read_buffer = |reader: &mut Box<dyn std::io::Read>| -> io::Result<Buffer> {
        let mut buf = new_buffer();
        reader.read(&mut buf);
        return Ok(buf);
    };

    let hash_buffer = |buf: Buffer| Hash::from_vec(buf);

    let iterate_over_buffers =
        |mut reader: Box<dyn Read>| -> Result<Box<dyn Iterator<Item = Buffer>>, io::Error> {
            Ok(Box::new(iter::from_fn(move || {
                match read_buffer(&mut reader) {
                    Ok(buf) if buf.is_empty() => None,
                    Ok(buf) => Some(buf),
                    Err(_) => None,
                }
            })))
        };

    let iterate_over_hashes = |reader: Box<dyn Read>| -> Result<
        Box<dyn Iterator<Item = (Hash, Vec<u8>)>>,
        Box<dyn Error>,
    > {
        Ok(Box::new(iterate_over_buffers(reader)?.map(|buf| {
            let hash = hash_buffer(buf.clone());
            (hash, buf)
        })))
    };

    let old_hashes = iterate_over_hashes(Box::new(old))?.collect::<Vec<_>>();
    let new_hashes = iterate_over_hashes(Box::new(new))?.collect::<Vec<_>>();

    let find_resync = |start: usize| -> Option<(usize, usize)> {
        const SPAN: usize = 3;

        let old_len = old_hashes.len();
        let new_len = new_hashes.len();

        for i in start..=old_len.saturating_sub(SPAN) {
            for j in start..=new_len.saturating_sub(SPAN) {
                if (0..SPAN).all(|k| old_hashes[i + k] == new_hashes[j + k]) {
                    return Some((i, j));
                }
            }
        }

        None
    };

    let next_equal_index = |start: usize| {
        (0..(old_hashes.len() - start).min(new_hashes.len() - start))
            .find(|&i| old_hashes[i].0 == new_hashes[i].0)
    };
    // let mut fragment: DiffFragment = DiffFragment::UNCHANGED { len: 0 };

    let mut fragments = Vec::new();

    // Buffers to accumulate added and deleted fragments
    let mut pending_added: Vec<u8> = Vec::new();
    let mut pending_deleted: usize = 0;

    let max_len = old_hashes.len().max(new_hashes.len());
    let mut cursor = 0;
    while cursor < max_len {
        let old_entry = old_hashes.get(cursor);
        let new_entry = new_hashes.get(cursor);

        match (old_entry, new_entry) {
            (Some((old_hash, old_data)), Some((new_hash, new_data))) => {
                if old_hash == new_hash {
                    if pending_deleted > 0 {
                        fragments.push(DiffFragment::DELETED {
                            len: pending_deleted * block_size,
                        });
                        pending_deleted = 0;
                    }
                    if !pending_added.is_empty() {
                        match fragments.last_mut() {
                            Some(DiffFragment::ADDED { body }) => {
                                body.extend_from_slice(&pending_added)
                            }
                            _ => fragments.push(DiffFragment::ADDED {
                                body: pending_added.clone(),
                            }),
                        }
                        pending_added.clear();
                    }
                    match fragments.last_mut() {
                        Some(DiffFragment::UNCHANGED { len }) => *len += old_data.len(),
                        _ => fragments.push(DiffFragment::UNCHANGED {
                            len: old_data.len(),
                        }),
                    }
                    cursor += 1;
                } else {
                    if let Some((old_index, new_index)) = find_resync(cursor) {
                        pending_deleted += old_index - cursor;
                        for (_, data) in new_hashes.iter().skip(cursor).take(new_index - cursor) {
                            pending_added.extend_from_slice(data);
                        }
                        if pending_deleted > 0 {
                            fragments.push(DiffFragment::DELETED {
                                len: pending_deleted * block_size,
                            });
                            pending_deleted = 0;
                        }
                        if !pending_added.is_empty() {
                            match fragments.last_mut() {
                                Some(DiffFragment::ADDED { body }) => {
                                    body.extend_from_slice(&pending_added)
                                }
                                _ => fragments.push(DiffFragment::ADDED {
                                    body: pending_added.clone(),
                                }),
                            }
                            pending_added.clear();
                        }
                        cursor = old_index.max(new_index);
                    } else {
                        pending_deleted += 1;
                        pending_added.extend_from_slice(new_data);
                        cursor += 1;
                    }
                }
            }
            (Some((_old_hash, _old_data)), None) => {
                pending_deleted += 1;
                cursor += 1;
            }
            (None, Some((_new_hash, new_data))) => {
                pending_added.extend_from_slice(new_data);
                cursor += 1;
            }
            (None, None) => break,
        }
    }
    if pending_deleted > 0 {
        fragments.push(DiffFragment::DELETED {
            len: pending_deleted * block_size,
        });
    }
    if !pending_added.is_empty() {
        match fragments.last_mut() {
            Some(DiffFragment::ADDED { body }) => body.extend_from_slice(&pending_added),
            _ => fragments.push(DiffFragment::ADDED {
                body: pending_added,
            }),
        }
    }

    Ok(())
}

