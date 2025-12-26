use std::error::Error;
use std::io::{Read, Write};
use std::{fmt, io, iter};

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
) -> Result<Vec<DiffFragment>, Box<dyn Error>> {
    type Buffer = Vec<u8>;

    let read_buffer = |reader: &mut Box<dyn std::io::Read>| -> io::Result<Buffer> {
        let mut buf = vec![0u8; block_size];
        let n = reader.read(&mut buf)?;
        buf.truncate(n);
        Ok(buf)
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

    let find_resync = |old_start: usize, new_start: usize| -> Option<(usize, usize)> {
        const SPAN: usize = 3;

        let old_len = old_hashes.len();
        let new_len = new_hashes.len();

        for i in old_start..=old_len.saturating_sub(SPAN) {
            for j in new_start..=new_len.saturating_sub(SPAN) {
                if (0..SPAN).all(|k| old_hashes[i + k].0 == new_hashes[j + k].0) {
                    return Some((i, j));
                }
            }
        }

        None
    };

    let mut fragments = Vec::new();

    // Buffers to accumulate added and deleted fragments
    let mut pending_added: Vec<u8> = Vec::new();
    let mut pending_deleted: usize = 0;

    let mut old_cursor = 0;
    let mut new_cursor = 0;
    
    while old_cursor < old_hashes.len() || new_cursor < new_hashes.len() {
        let old_entry = old_hashes.get(old_cursor);
        let new_entry = new_hashes.get(new_cursor);

        match (old_entry, new_entry) {
            (Some((old_hash, old_data)), Some((new_hash, new_data))) => {
                if old_hash == new_hash {
                    // Flush pending deletions
                    if pending_deleted > 0 {
                        fragments.push(DiffFragment::DELETED {
                            len: pending_deleted * block_size,
                        });
                        pending_deleted = 0;
                    }
                    // Flush pending additions
                    if !pending_added.is_empty() {
                        fragments.push(DiffFragment::ADDED {
                            body: pending_added.clone(),
                        });
                        pending_added.clear();
                    }
                    // Now merge or add UNCHANGED
                    match fragments.last_mut() {
                        Some(DiffFragment::UNCHANGED { len }) => *len += old_data.len(),
                        _ => fragments.push(DiffFragment::UNCHANGED {
                            len: old_data.len(),
                        }),
                    }
                    old_cursor += 1;
                    new_cursor += 1;
                } else {
                    if let Some((old_index, new_index)) = find_resync(old_cursor, new_cursor) {
                        // Add deleted blocks from old
                        pending_deleted += old_index - old_cursor;
                        // Add added blocks from new
                        for (_, data) in new_hashes.iter().skip(new_cursor).take(new_index - new_cursor) {
                            pending_added.extend_from_slice(data);
                        }
                        if pending_deleted > 0 {
                            fragments.push(DiffFragment::DELETED {
                                len: pending_deleted * block_size,
                            });
                            pending_deleted = 0;
                        }
                        if !pending_added.is_empty() {
                            fragments.push(DiffFragment::ADDED {
                                body: pending_added.clone(),
                            });
                            pending_added.clear();
                        }
                        // Jump both cursors to their respective resync points
                        old_cursor = old_index;
                        new_cursor = new_index;
                    } else {
                        // No resync found - consume both as changed
                        pending_deleted += 1;
                        pending_added.extend_from_slice(new_data);
                        old_cursor += 1;
                        new_cursor += 1;
                    }
                }
            }
            (Some((_old_hash, _old_data)), None) => {
                pending_deleted += 1;
                old_cursor += 1;
            }
            (None, Some((_new_hash, new_data))) => {
                pending_added.extend_from_slice(new_data);
                new_cursor += 1;
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
        fragments.push(DiffFragment::ADDED {
            body: pending_added,
        });
    }

    Ok(fragments)
}

pub fn apply_diff<R: Read, W: Write>(
    mut old: R,
    fragments: &[DiffFragment],
    output: &mut W,
) -> io::Result<()> {
    for fragment in fragments {
        match fragment {
            DiffFragment::ADDED { body } => {
                output.write_all(body)?;
            }
            DiffFragment::UNCHANGED { len } => {
                let mut buffer = vec![0u8; *len];
                old.read_exact(&mut buffer)?;
                output.write_all(&buffer)?;
            }
            DiffFragment::DELETED { len: _ } => {
                // Skip deleted bytes from old
                // We don't need to do anything here
            }
        }
    }
    Ok(())
}

