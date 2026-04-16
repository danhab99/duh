use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt::Display;
use std::io;
use std::io::{Read, Seek};

use crate::diff::DiffFragment;
use crate::hash::Hash;
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

fn iterate_cdc_rewind<R: Read + Seek, F: FnMut(Position, Hash) -> Option<()>>(
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
    let mut chunk_start = 0usize;

    let mut strong_buf = Vec::<u8>::new();

    loop {
        let (old_chunk, old_eof) = read_chunk(old, window)?;
        if old_chunk.is_empty() || old_eof {
            break;
        }

        let mut chunk_offset = 0usize;
        while chunk_offset < old_chunk.len() {
            let slice = &old_chunk[chunk_offset..];

            if let Some(boundary) = hasher.next_match(slice, hash_mod) {
                let boundary_pos = chunk_offset + boundary;
                strong_buf.extend_from_slice(&old_chunk[chunk_offset..boundary_pos]);
                chunks_found += 1;
                let strong_hash = Hash::digest_slice(&strong_buf)?;
                vlog!(
                    "dedup::iterate_cdc_rewind: found chunk boundary at offset={}, chunk_len={}, chunk_start={}",
                    offset + boundary_pos,
                    strong_buf.len(),
                    chunk_start
                );
                if action(
                    Position {
                        start: chunk_start,
                        length: strong_buf.len(),
                    },
                    strong_hash,
                )
                .is_none()
                {
                    vlog!("dedup::iterate_cdc_rewind: action returned None, stopping iteration");
                    return Ok(());
                }

                strong_buf.clear();
                hasher = gearhash::Hasher::default();
                chunk_start = offset + boundary_pos;
                chunk_offset = boundary_pos;
            } else {
                strong_buf.extend_from_slice(slice);
                break;
            }
        }

        offset += old_chunk.len();

        if old_eof {
            vlog!("dedup::iterate_cdc_rewind: reached EOF");
            break;
        }
    }

    // Handle final chunk if there's remaining data
    if !strong_buf.is_empty() {
        chunks_found += 1;
        let strong_hash = Hash::digest_slice(&strong_buf)?;
        vlog!(
            "dedup::iterate_cdc_rewind: processing final chunk at chunk_start={}, chunk_len={}",
            chunk_start,
            strong_buf.len()
        );
        action(
            Position {
                start: chunk_start,
                length: strong_buf.len(),
            },
            strong_hash,
        );
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

fn normalize_hash_mod(hash_mod: u64) -> u64 {
    if hash_mod == 0 {
        return 0;
    }

    // Gearhash expects a mask of the form 2^n - 1. If callers pass a byte
    // length instead, convert it to the nearest lower power-of-two mask.
    if hash_mod & (hash_mod + 1) == 0 {
        return hash_mod;
    }

    let next_pow2 = hash_mod.next_power_of_two();
    if next_pow2 <= 1 {
        return 0;
    }

    (next_pow2 >> 1).saturating_sub(1)
}

pub fn build_cdc_rewind<R: Read + Seek>(
    old: &mut R,
    window: usize,
    hash_mod: u64,
) -> Result<HashMap<Hash, Position>, Box<dyn std::error::Error>> {
    vlog!("dedup::build_cdc_rewind: starting with window={}", window);
    let hash_mod = normalize_hash_mod(hash_mod);
    vlog!(
        "dedup::build_cdc_rewind: normalized hash_mod mask={}",
        hash_mod
    );

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

/// Progress events emitted during the two CDC indexing phases.
pub enum DedupProgress {
    /// A chunk boundary was found while scanning the old (baseline) stream.
    OldChunk {
        /// Sequential discovery index (0-based).
        index: usize,
        /// Byte length of this chunk.
        len: usize,
    },
    /// A chunk boundary was found while scanning the new stream.
    NewChunk {
        /// Sequential discovery index in the new stream (0-based).
        index: usize,
        /// Byte length of this chunk.
        len: usize,
        /// If this chunk also appears in the old stream, the sequential
        /// discovery index of that matching old chunk.  `None` means the
        /// chunk is brand-new (an addition).
        old_index: Option<usize>,
    },
}

// 1. Define a struct to hold the iterator's state
pub struct DedupeFragIterator<R: Read + Seek> {
    deleted: VecDeque<Position>,
    unchanged: VecDeque<Position>,
    added: VecDeque<Position>,
    new: R,
}

impl<R: Read + Seek> DedupeFragIterator<R> {
    pub fn build<P: FnMut(DedupProgress)>(
        mut old: R,
        mut new: R,
        window: usize,
        hash_mod: u64,
        mut progress: Option<P>,
    ) -> Result<Self, Box<dyn Error>> {
        let hash_mod = normalize_hash_mod(hash_mod);

        // ── Phase 1: index the old (baseline) stream ──────────────────────
        let mut old_map = HashMap::<Hash, Position>::new();
        // Maps hash → sequential discovery index for matched-color reporting.
        let mut old_index_map = HashMap::<Hash, usize>::new();
        let mut old_chunk_idx = 0usize;

        iterate_cdc_rewind(&mut old, window, hash_mod, |position, hash| {
            let should_insert = match old_map.get(&hash) {
                Some(existing) => (existing.start + existing.length) > position.start,
                None => true,
            };
            if should_insert {
                old_map.insert(hash, position);
                old_index_map.insert(hash, old_chunk_idx);
            }
            if let Some(ref mut p) = progress {
                p(DedupProgress::OldChunk {
                    index: old_chunk_idx,
                    len: position.length,
                });
            }
            old_chunk_idx += 1;
            Some(())
        })?;

        // ── Phase 2: index the new stream ─────────────────────────────────
        let mut new_map = HashMap::<Hash, Position>::new();
        let mut new_chunk_idx = 0usize;

        iterate_cdc_rewind(&mut new, window, hash_mod, |position, hash| {
            let should_insert = match new_map.get(&hash) {
                Some(existing) => (existing.start + existing.length) > position.start,
                None => true,
            };
            if should_insert {
                new_map.insert(hash, position);
            }
            let old_index = old_index_map.get(&hash).copied();
            if let Some(ref mut p) = progress {
                p(DedupProgress::NewChunk {
                    index: new_chunk_idx,
                    len: position.length,
                    old_index,
                });
            }
            new_chunk_idx += 1;
            Some(())
        })?;

        // ── Phase 3: categorise chunks into deleted / unchanged / added ────
        let mut deleted = VecDeque::<Position>::new();
        let mut unchanged = VecDeque::<Position>::new();
        let mut added = VecDeque::<Position>::new();

        for (key, value) in &old_map {
            if new_map.get(key).is_none() {
                deleted.push_back(*value);
            } else {
                unchanged.push_back(*value);
            }
        }

        for (key, value) in &new_map {
            if old_map.get(key).is_none() {
                added.push_back(*value);
            }
        }

        vlog!(
            "dedup::build: all positions categorized deleted={} unchanged={} added={}",
            deleted.len(),
            unchanged.len(),
            added.len(),
        );

        Ok(Self {
            deleted,
            unchanged,
            added,
            new,
        })
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

pub fn build_diff_fragments<R: Read + Seek, P: FnMut(DedupProgress)>(
    old: R,
    new: R,
    window: usize,
    hash_mod: u64,
    progress: Option<P>,
) -> Result<DedupeFragIterator<R>, Box<dyn Error>> {
    Ok(DedupeFragIterator::<R>::build(old, new, window, hash_mod, progress)?)
}
