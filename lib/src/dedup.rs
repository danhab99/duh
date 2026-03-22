use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt::Display;
use std::io;
use std::io::{Read, Seek};

use blake3::Hash;

use crate::diff::DiffFragment;
use crate::vlog;

fn read_chunk<R: Read>(reader: &mut R, size: usize) -> std::io::Result<(Vec<u8>, bool)> {
    if size == 0 {
        return Ok((vec![0u8; 0], false));
    }
    let mut buf = vec![0u8; size];
    let n = reader.read(&mut buf)?;

    if n == 0 {
        Ok((Vec::new(), true))
    } else {
        buf.truncate(n);
        Ok((buf, false))
    }
}

fn iterate_cdc_rewind<R: Read + Seek, F: FnMut(Position, blake3::Hash) -> Option<()>>(
    old: &mut R,
    window: usize,
    hash_mod: u64,
    mut action: F,
) -> Result<(), Box<dyn std::error::Error>> {
    vlog!(
        "dedup::iterate_cdc_rewind: starting iteration with window={}",
        window
    );
    let old_starting_pos = old.stream_position()?;

    let mut hasher = gearhash::Hasher::default();
    let mut offset = 0usize;
    let mut chunks_found = 0;

    let mut strong_buf = Vec::<u8>::new();

    loop {
        let (old_chunk, old_eof) = read_chunk(old, window)?;
        if old_chunk.is_empty() {
            break;
        }
        strong_buf.append(&mut old_chunk.clone());

        // loop through all matches, and push the corresponding chunks
        if let Some(boundary) = hasher.next_match(&old_chunk, hash_mod) {
            chunks_found += 1;
            let strong_hash = blake3::hash(&strong_buf);
            vlog!(
                "dedup::iterate_cdc_rewind: found chunk boundary at offset={}, chunk_len={}",
                offset + boundary,
                strong_buf.len()
            );
            if let None = action(
                Position {
                    start: offset + boundary,
                    length: strong_buf.len(),
                },
                strong_hash,
            ) {
                vlog!("dedup::iterate_cdc_rewind: action returned None, stopping iteration");
                break;
            };
            strong_buf.clear();
            hasher = gearhash::Hasher::default();
        }

        offset += old_chunk.len();

        if old_eof {
            vlog!("dedup::iterate_cdc_rewind: reached EOF");
            break;
        }
    }

    old.seek(io::SeekFrom::Start(old_starting_pos))?;
    vlog!(
        "dedup::iterate_cdc_rewind: completed, found {} chunks",
        chunks_found
    );

    Ok(())
}

#[derive(PartialEq, PartialOrd, Copy, Clone)]
pub struct Position {
    start: usize,
    length: usize,
}

impl Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Position {{ start: {}, length: {} }}",
            self.start, self.length
        )
    }
}

pub fn build_cdc_rewind<R: Read + Seek>(
    old: &mut R,
    window: usize,
    hash_mod: u64,
) -> Result<HashMap<Hash, Position>, Box<dyn std::error::Error>> {
    vlog!("dedup::build_cdc_rewind: starting with window={}", window);

    let mut map = HashMap::<Hash, Position>::new();

    iterate_cdc_rewind(old, window, hash_mod, |position, hash| {
        if match map.get(&hash) {
            Some(existing_position) => {
                (existing_position.start + existing_position.length) > position.start
            }
            None => true,
        } {
            vlog!(
                "dedup::build_cdc_rewind: found chunk at position={} hash={}",
                position,
                hash.to_hex(),
            );
            map.insert(hash, position);
        }

        Some(())
    })?;

    vlog!(
        "dedup::build_cdc_rewind: completed with {} chunks",
        map.len()
    );

    vlog!("===== OLD CDC INDEX =====");
    for (hash, position) in map.iter() {
        vlog!("  position={} hash={}", position, hash.to_hex(),);
    }
    vlog!("=========================");

    Ok(map)
}

// 1. Define a struct to hold the iterator's state
pub struct DedupeFragIterator<R: Read + Seek> {
    deleted: VecDeque<Position>,
    unchanged: VecDeque<Position>,
    added: VecDeque<Position>,
    new: R,
}

impl<R: Read + Seek> DedupeFragIterator<R> {
    pub fn build(
        mut old: R,
        mut new: R,
        window: usize,
        hash_mod: u64,
    ) -> Result<Self, Box<dyn Error>> {
        let old_cdc = &build_cdc_rewind(&mut old, window, hash_mod)?;
        let new_cdc = &build_cdc_rewind(&mut new, window, hash_mod)?;

        let mut deleted = VecDeque::<Position>::new();
        let mut unchanged = VecDeque::<Position>::new();
        let mut added = VecDeque::<Position>::new();

        for (key, value) in old_cdc {
            if let None = new_cdc.get(&key) {
                deleted.push_back(*value);
            } else {
                unchanged.push_back(*value);
            }
        }

        for (key, value) in new_cdc {
            if let None = old_cdc.get(&key) {
                added.push_back(*value);
            }
        }

        vlog!(
            "dedup::build: all positions categorized deleted={} unchanged={} added={}",
            deleted.len(),
            unchanged.len(),
            added.len(),
        );

        return Ok(Self {
            deleted,
            unchanged,
            added,
            new,
        });
    }
}

// 2. Implement the Iterator trait
impl<R: Read + Seek> Iterator for DedupeFragIterator<R> {
    // Specify the type of items the iterator produces
    type Item = DiffFragment;

    // The next() method is where the lazy logic resides
    fn next(&mut self) -> Option<Self::Item> {
        enum SelectedSet {
            Deleted,
            Unchanged,
            Added,
        }

        // Find which VecDeque has the earliest position at the front,
        // returning the position and the id of the VecDeque set
        let mut earliest: Option<(SelectedSet, &Position)> = None;

        for (set_id, deq) in [
            (SelectedSet::Deleted, &self.deleted),
            (SelectedSet::Unchanged, &self.unchanged),
            (SelectedSet::Added, &self.added),
        ] {
            if let Some(pos) = deq.front() {
                match earliest {
                    None => earliest = Some((set_id, pos)),
                    Some((_, best_pos)) if pos.start < best_pos.start => {
                        earliest = Some((set_id, pos))
                    }
                    _ => {}
                }
            }
        }

        match earliest {
            Some((selected, _)) => Some(match selected {
                SelectedSet::Deleted => {
                    let d = self.deleted.pop_front().unwrap();
                    vlog!("dedup::next: yielding deleted {}", d.length);
                    DiffFragment::DELETED { len: d.length }
                }

                SelectedSet::Unchanged => {
                    let d = self.unchanged.pop_front().unwrap();
                    vlog!("dedup::next: yielding unchanged {}", d.length);
                    DiffFragment::UNCHANGED { len: d.length }
                }
                SelectedSet::Added => {
                    let d = self.added.pop_front().unwrap();
                    self.new.seek(io::SeekFrom::Start(d.start as u64)).unwrap();
                    let (body, _) = read_chunk(&mut self.new, d.length).unwrap();

                    vlog!("dedup::next: yielding added {}", d.length);
                    DiffFragment::ADDED { body }
                }
            }),
            None => None,
        }
    }
}

pub fn build_diff_fragments<R: Read + Seek>(
    old: R,
    new: R,
    window: usize,
    hash_mod: u64,
) -> Result<DedupeFragIterator<R>, Box<dyn Error>> {
    Ok(DedupeFragIterator::<R>::build(old, new, window, hash_mod)?)
}
