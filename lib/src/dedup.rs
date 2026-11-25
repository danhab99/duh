use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt::Display;
use std::io;
use std::io::{Read, Seek};

use crate::diff::DiffFragment;
use crate::hash::Hash;
use crate::utils::read_chunk;
use crate::vlog;

/// Split `old` into content-defined chunks and call `action` with each
/// chunk's position, hash and bytes, in order. A chunk never grows past
/// `max_chunk` bytes (0 means no limit): runs that never hit a boundary,
/// such as long stretches of identical bytes, are cut at that length.
fn iterate_cdc_rewind<R: Read + Seek, F: FnMut(Position, Hash, &[u8]) -> Option<()>>(
    old: &mut R,
    window: usize,
    hash_mod: u64,
    max_chunk: usize,
    mut action: F,
) -> Result<(), Box<dyn std::error::Error>> {
    let max_chunk = if max_chunk == 0 { usize::MAX } else { max_chunk };
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
            // Never look past the point where the chunk would be full.
            let room = max_chunk - strong_buf.len();
            let end = old_chunk.len().min(chunk_offset.saturating_add(room));
            let slice = &old_chunk[chunk_offset..end];

            let cut = match hasher.next_match(slice, hash_mod) {
                Some(boundary) => Some(chunk_offset + boundary),
                None if strong_buf.len() + slice.len() == max_chunk => Some(end),
                None => None,
            };

            if let Some(boundary_pos) = cut {
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
                    &strong_buf,
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
            &strong_buf,
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

    iterate_cdc_rewind(old, window, hash_mod, 0, |position, hash, _| {
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

/// One step of the edit script that turns the old stream into the new one.
/// Steps are applied in order: UNCHANGED copies from the old stream, DELETED
/// skips old bytes, ADDED emits new bytes (read lazily from the new stream).
enum EditOp {
    Deleted(usize),
    Unchanged(usize),
    Added { start: usize, len: usize },
}

fn push_op(ops: &mut VecDeque<EditOp>, op: EditOp) {
    match (ops.back_mut(), &op) {
        (Some(EditOp::Deleted(a)), EditOp::Deleted(b)) => *a += b,
        (Some(EditOp::Unchanged(a)), EditOp::Unchanged(b)) => *a += b,
        (Some(EditOp::Added { start, len }), EditOp::Added { start: s, len: l })
            if *start + *len == *s =>
        {
            *len += l
        }
        _ => ops.push_back(op),
    }
}

pub struct DedupeFragIterator<R: Read + Seek> {
    ops: VecDeque<EditOp>,
    new: R,
}

impl<R: Read + Seek> DedupeFragIterator<R> {
    pub fn build<P: FnMut(DedupProgress)>(
        mut old: R,
        mut new: R,
        window: usize,
        hash_mod: u64,
        max_chunk: usize,
        mut progress: Option<P>,
    ) -> Result<Self, Box<dyn Error>> {
        let hash_mod = normalize_hash_mod(hash_mod);

        // ── Phase 1: index the old (baseline) stream ──────────────────────
        // Maps hash → every start offset of that chunk in old (ascending).
        let mut old_starts = HashMap::<Hash, Vec<usize>>::new();
        // Maps hash → sequential discovery index for matched-color spacerting.
        let mut old_index_map = HashMap::<Hash, usize>::new();
        let mut old_chunk_idx = 0usize;
        let mut old_len = 0usize;

        iterate_cdc_rewind(&mut old, window, hash_mod, max_chunk, |position, hash, _| {
            old_starts.entry(hash).or_default().push(position.start);
            old_index_map.entry(hash).or_insert(old_chunk_idx);
            old_len = position.start + position.length;
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
        let mut new_chunks = Vec::<(Position, Hash)>::new();
        let mut new_chunk_idx = 0usize;

        iterate_cdc_rewind(&mut new, window, hash_mod, max_chunk, |position, hash, _| {
            new_chunks.push((position, hash));
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

        // ── Phase 3: build an ordered edit script ──────────────────────────
        // Walk new chunks in order, matching each against the earliest old
        // chunk at or after the old cursor. Skipped old bytes are DELETED,
        // unmatched new chunks are ADDED. Within a replaced region the
        // deletion is emitted before the addition.
        let mut ops = VecDeque::<EditOp>::new();
        let mut old_cursor = 0usize;
        let mut pending_added = VecDeque::<EditOp>::new();

        for (position, hash) in &new_chunks {
            let matched = old_starts.get(hash).and_then(|starts| {
                let i = starts.partition_point(|&s| s < old_cursor);
                starts.get(i).copied()
            });

            match matched {
                Some(old_start) => {
                    if old_start > old_cursor {
                        push_op(&mut ops, EditOp::Deleted(old_start - old_cursor));
                    }
                    for op in pending_added.drain(..) {
                        push_op(&mut ops, op);
                    }
                    push_op(&mut ops, EditOp::Unchanged(position.length));
                    old_cursor = old_start + position.length;
                }
                None => push_op(
                    &mut pending_added,
                    EditOp::Added {
                        start: position.start,
                        len: position.length,
                    },
                ),
            }
        }

        if old_len > old_cursor {
            push_op(&mut ops, EditOp::Deleted(old_len - old_cursor));
        }
        for op in pending_added.drain(..) {
            push_op(&mut ops, op);
        }

        vlog!("dedup::build: edit script has {} ops", ops.len());

        Ok(Self { ops, new })
    }
}

impl<R: Read + Seek> Iterator for DedupeFragIterator<R> {
    type Item = DiffFragment;

    fn next(&mut self) -> Option<Self::Item> {
        Some(match self.ops.pop_front()? {
            EditOp::Deleted(len) => {
                vlog!("dedup::next: yielding deleted {}", len);
                DiffFragment::DELETED { len }
            }
            EditOp::Unchanged(len) => {
                vlog!("dedup::next: yielding unchanged {}", len);
                DiffFragment::UNCHANGED { len }
            }
            EditOp::Added { start, len } => {
                self.new.seek(io::SeekFrom::Start(start as u64)).unwrap();
                let mut body = vec![0u8; len];
                self.new.read_exact(&mut body).unwrap();
                vlog!("dedup::next: yielding added {}", len);
                DiffFragment::ADDED { body }
            }
        })
    }
}

pub fn build_diff_fragments<R: Read + Seek, P: FnMut(DedupProgress)>(
    old: R,
    new: R,
    window: usize,
    hash_mod: u64,
    max_chunk: usize,
    progress: Option<P>,
) -> Result<DedupeFragIterator<R>, Box<dyn Error>> {
    Ok(DedupeFragIterator::<R>::build(old, new, window, hash_mod, max_chunk, progress)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Deterministic pseudo-random bytes; equal seeds give equal streams.
    fn noise(seed: u64, len: usize) -> Vec<u8> {
        let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect()
    }

    /// Diff old → new, then replay the fragments onto old the way
    /// LazyFileReplay does (sequentially). Returns (rebuilt, unchanged bytes).
    fn round_trip(old: &[u8], new: &[u8]) -> (Vec<u8>, usize) {
        let frags = build_diff_fragments(
            Cursor::new(old.to_vec()),
            Cursor::new(new.to_vec()),
            4096,
            4096,
            MAX_CHUNK,
            None::<fn(DedupProgress)>,
        )
        .unwrap();

        let mut out = Vec::new();
        let mut cursor = 0usize;
        let mut unchanged = 0usize;
        for frag in frags {
            match frag {
                DiffFragment::UNCHANGED { len } => {
                    out.extend_from_slice(&old[cursor..cursor + len]);
                    cursor += len;
                    unchanged += len;
                }
                DiffFragment::DELETED { len } => cursor += len,
                DiffFragment::ADDED { body } => out.extend_from_slice(&body),
            }
        }
        assert!(cursor <= old.len(), "fragments ran past the end of old");
        (out, unchanged)
    }

    fn cat(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    /// Rebuilds `new` exactly and reuses at least 90% of `shared` bytes.
    fn check(old: &[u8], new: &[u8], shared: usize) {
        let (rebuilt, unchanged) = round_trip(old, new);
        assert!(rebuilt == new, "rebuilt file differs from new");
        assert!(
            unchanged * 10 >= shared * 9,
            "only {} of {} shared bytes were deduplicated",
            unchanged,
            shared
        );
    }

    const N: usize = 200_000;
    const MAX_CHUNK: usize = 8 * 4096;

    #[test]
    fn identical() {
        let a = noise(1, N);
        check(&a, &a, N);
    }

    #[test]
    fn from_and_to_empty() {
        let a = noise(1, N);
        check(&[], &a, 0);
        check(&a, &[], 0);
        check(&[], &[], 0);
    }

    #[test]
    fn append_and_trim() {
        let (a, b) = (noise(1, N), noise(2, N / 2));
        check(&a, &cat(&[&a, &b]), N);
        check(&cat(&[&a, &b]), &a, N);
    }

    #[test]
    fn prepend_and_drop_prefix() {
        let (a, b) = (noise(1, N), noise(2, N / 2));
        check(&a, &cat(&[&b, &a]), N);
        check(&cat(&[&b, &a]), &a, N);
    }

    #[test]
    fn insert_and_remove_middle() {
        let (a, b) = (noise(1, N), noise(2, N / 2));
        let (head, tail) = a.split_at(N / 2);
        check(&a, &cat(&[head, &b, tail]), N);
        check(&cat(&[head, &b, tail]), &a, N);
    }

    #[test]
    fn replace_middle() {
        let (a, b) = (noise(1, N), noise(2, N / 3));
        let (head, rest) = a.split_at(N / 3);
        let (_, tail) = rest.split_at(N / 3);
        check(&a, &cat(&[head, &b, tail]), head.len() + tail.len());
    }

    #[test]
    fn unrelated() {
        check(&noise(1, N), &noise(2, N), 0);
    }

    /// Runs with no content boundary (here, all zeros) are cut at
    /// MAX_CHUNK, so an edit inside one only re-adds the chunks around it.
    #[test]
    fn uniform_runs_are_cut_at_max_chunk() {
        let old = vec![0u8; 4 * 1024 * 1024];
        let mut new = old.clone();
        new[2 * 1024 * 1024] = b'X';
        let (rebuilt, unchanged) = round_trip(&old, &new);
        assert!(rebuilt == new);
        assert!(
            new.len() - unchanged <= 2 * MAX_CHUNK,
            "a 1-byte edit re-added {} bytes",
            new.len() - unchanged
        );

        let mut sizes = Vec::new();
        iterate_cdc_rewind(&mut Cursor::new(&old), 4096, 4095, MAX_CHUNK, |p, _, bytes| {
            assert_eq!(p.length, bytes.len());
            sizes.push(p.length);
            Some(())
        })
        .unwrap();
        assert_eq!(sizes.iter().sum::<usize>(), old.len());
        assert!(sizes.iter().all(|&n| n <= MAX_CHUNK));
    }

    #[test]
    fn moved_block_still_rebuilds() {
        // Swapping halves can't be expressed as forward matches only; the
        // result must still be exact even if part is re-added.
        let a = noise(1, N);
        let (x, y) = a.split_at(N / 2);
        let (rebuilt, _) = round_trip(&a, &cat(&[y, x]));
        assert!(rebuilt == cat(&[y, x]));
    }
}
