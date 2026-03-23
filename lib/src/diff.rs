use std::collections::HashMap;
use std::io::{Read, Seek};
use std::ops::Index;
use std::{fmt, io, process};

use crate::hash::Hash;

use crate::vlog;

/// Minimum window size for matching - prevents spurious matches on short byte sequences
const MIN_WINDOW: usize = 64;

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

const HASH_MOD: u64 = 1024;

type Position = usize;

fn iterate_cdc_rewind<R: Read + Seek, F: FnMut(Position, &[u8], Hash) -> Option<()>>(
    old: &mut R,
    window: usize,
    mut action: F,
) -> Result<(), Box<dyn std::error::Error>> {
    vlog!(
        "diff::iterate_cdc_rewind: starting iteration with window={}",
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
        if old_chunk.is_empty() {
            break;
        }
        strong_buf.extend_from_slice(&old_chunk);

        // loop through all matches, and push the corresponding chunks
        if let Some(boundary) = hasher.next_match(&old_chunk, HASH_MOD) {
            chunks_found += 1;
            let strong_hash = Hash::digest_slice(&strong_buf)?;
            vlog!(
                "diff::iterate_cdc_rewind: found chunk boundary at offset={}, chunk_len={}, chunk_start={}",
                offset + boundary,
                strong_buf.len(),
                chunk_start
            );
            if let None = action(chunk_start, strong_buf.as_slice(), strong_hash) {
                vlog!("diff::iterate_cdc_rewind: action returned None, stopping iteration");
                break;
            };
            strong_buf.clear();
            hasher = gearhash::Hasher::default();
            chunk_start = offset + boundary;
        }

        offset += old_chunk.len();

        if old_eof {
            vlog!("diff::iterate_cdc_rewind: reached EOF");
            break;
        }
    }

    // Handle final chunk if there's remaining data
    if !strong_buf.is_empty() {
        chunks_found += 1;
        let strong_hash = Hash::digest_slice(&strong_buf)?;
        vlog!(
            "diff::iterate_cdc_rewind: processing final chunk at chunk_start={}, chunk_len={}",
            chunk_start,
            strong_buf.len()
        );
        action(chunk_start, strong_buf.as_slice(), strong_hash);
    }

    old.seek(io::SeekFrom::Start(old_starting_pos))?;
    vlog!(
        "diff::iterate_cdc_rewind: completed, found {} chunks",
        chunks_found
    );

    Ok(())
}

pub fn build_cdc_rewind<R: Read + Seek>(
    old: &mut R,
    window: usize,
) -> Result<HashMap<Hash, (Position, Vec<u8>)>, Box<dyn std::error::Error>> {
    vlog!("diff::build_cdc_rewind: starting with window={}", window);

    let mut map = HashMap::<Hash, (Position, Vec<u8>)>::new();

    iterate_cdc_rewind(old, window, |position, body, hash| {
        if match map.get(&hash) {
            Some((got_position, _)) => *got_position > position,
            None => true,
        } {
            vlog!(
                "diff::build_cdc_rewind: found chunk at position={}, len={} hash={}",
                position,
                body.len(),
                hash.to_hex(),
            );
            map.insert(hash, (position, Vec::from(body)));
        } else {
            vlog!(
                "diff::build_cdc_rewind: SKIPPING position={}, len={} hash={}",
                position,
                body.len(),
                hash.to_hex(),
            );
        }

        Some(())
    })?;

    vlog!(
        "diff::build_cdc_rewind: completed with {} chunks",
        map.len()
    );

    vlog!("===== OLD CDC INDEX =====");
    for (hash, (position, body)) in map.iter() {
        vlog!(
            "  position={} hash={} body={}",
            position,
            hash.to_hex(),
            String::from_utf8(body.clone())?
                .chars()
                .take(10)
                .collect::<String>()
        );
    }
    vlog!("=========================");

    Ok(map)
}

pub fn collect_divergence<R: Read + Seek>(
    old: &mut R,
    new: &mut R,
    window: usize,
    max_bytes: usize,
) -> Result<(usize, Vec<u8>), Box<dyn std::error::Error>> {
    vlog!(
        "diff::collect_divergence: starting with window={}, max_bytes={}",
        window,
        max_bytes
    );
    // Returns: (deleted_bytes, added_bytes)
    let old_starting_pos = old.stream_position()?;

    let old_cdc_index = build_cdc_rewind(old, window)?;

    let mut converging_old_position = 0usize;
    let mut converging_new_position = 0usize;

    let mut added_bytes = Vec::<u8>::new();

    iterate_cdc_rewind(new, window, |new_position, body, hash| {
        let mut b = Vec::from(body);
        added_bytes.append(&mut b);

        vlog!(
            "diff::collect_divergence: searching for convergence new_pos={} new_hash={}",
            new_position,
            hash.to_string()
        );

        match old_cdc_index.get(&hash) {
            Some((old_position, _)) => {
                converging_old_position = *old_position;
                converging_new_position = new_position;
                vlog!(
                    "diff::collect_divergence: found convergence at old_pos={}, new_pos={}",
                    converging_old_position,
                    converging_new_position
                );

                if added_bytes.len() < max_bytes {
                    Some(())
                } else {
                    None
                }
            }
            _ => None,
        }
    })?;

    let deleted_bytes = converging_old_position - (old_starting_pos as usize);
    vlog!(
        "diff::collect_divergence: completed with deleted={}, added={}",
        deleted_bytes,
        added_bytes.len()
    );

    return Ok((deleted_bytes, added_bytes));
}

fn collect_convergence<R: Read + Seek>(
    old: &mut R,
    new: &mut R,
    window: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
    if window < 1 {
        vlog!(
            "diff::collect_convergence: error - window too small: {}",
            window
        );
        return Err("window is too small".into());
    }

    vlog!("diff::collect_convergence: starting with window={}", window);
    let old_starting_pos = old.stream_position()?;
    let new_starting_pos = new.stream_position()?;

    let mut unchanged_len = 0usize;
    let mut hit_eof = false;

    loop {
        let (old_buf, old_eof) = read_chunk(old, window)?;
        let (new_buf, new_eof) = read_chunk(new, window)?;

        if old_eof || new_eof {
            hit_eof = true;
        }

        // Direct byte comparison
        if old_buf != new_buf {
            vlog!(
                "diff::collect_convergence: divergence found at unchanged_len={}",
                unchanged_len
            );
            // Found divergence, rewind to before this chunk
            old.seek(io::SeekFrom::Start(old_starting_pos + unchanged_len as u64))?;
            new.seek(io::SeekFrom::Start(new_starting_pos + unchanged_len as u64))?;
            break;
        }

        unchanged_len += old_buf.len();

        if hit_eof {
            vlog!(
                "diff::collect_convergence: reached EOF with unchanged_len={}",
                unchanged_len
            );
            return Ok(unchanged_len);
        }
    }

    if unchanged_len == 0 {
        vlog!("diff::collect_convergence: immediate divergence detected");
        // Files diverge immediately at this position - that's fine, return 0
        old.seek(io::SeekFrom::Start(old_starting_pos))?;
        new.seek(io::SeekFrom::Start(new_starting_pos))?;
        return Ok(0);
    }

    vlog!(
        "diff::collect_convergence: completed with unchanged_len={}",
        unchanged_len
    );
    Ok(unchanged_len)
}

fn test_eof<R: Read + Seek>(reader: &mut R) -> Result<bool, Box<dyn std::error::Error>> {
    let p = reader.stream_position()?;
    let (c, eof) = read_chunk(reader, 1)?;
    if c.len() > 0 {
        reader.seek(io::SeekFrom::Start(p))?;
    }
    return Ok(eof);
}

/// Iterator that yields diff fragments one at a time
pub struct DiffFragmentIter<R: Read + Seek> {
    old: R,
    new: R,
    window: usize,
    /// Maximum body size for a single ADDED fragment emitted by this iterator.
    max_fragment_size: usize,
    done: bool,
    pending: Option<DiffFragment>, // For consolidation
    queue: Vec<DiffFragment>,      // Queue for fragments from divergence
}

impl<R: Read + Seek> DiffFragmentIter<R> {
    pub fn new(old: R, new: R, window: usize, max_fragment_size: usize) -> Self {
        vlog!(
            "diff::DiffFragmentIter::new: window={}, max_fragment_size={}",
            window,
            max_fragment_size
        );
        Self {
            old,
            new,
            window,
            max_fragment_size,
            done: false,
            pending: None,
            queue: Vec::new(),
        }
    }

    fn next_raw(&mut self) -> Option<Result<DiffFragment, Box<dyn std::error::Error>>> {
        // First drain the queue
        if !self.queue.is_empty() {
            vlog!("diff::DiffFragmentIter::next_raw: returning queued fragment");
            return Some(Ok(self.queue.remove(0)));
        }

        if self.done {
            vlog!("diff::DiffFragmentIter::next_raw: iteration complete");
            return None;
        }

        // Check EOF on both
        let old_eof = match test_eof(&mut self.old) {
            Ok(eof) => eof,
            Err(e) => return Some(Err(e)),
        };
        let new_eof = match test_eof(&mut self.new) {
            Ok(eof) => eof,
            Err(e) => return Some(Err(e)),
        };

        if old_eof && new_eof {
            vlog!("diff::DiffFragmentIter::next_raw: both files reached EOF");
            self.done = true;
            return None;
        }

        // If only one is EOF, handle remaining
        if new_eof {
            vlog!("diff::DiffFragmentIter::next_raw: new file EOF, processing remaining old file");
            // Only the byte count is needed for DELETED; avoid materialising old content.
            let cur = match self.old.stream_position() {
                Ok(p) => p,
                Err(e) => return Some(Err(e.into())),
            };
            let end = match self.old.seek(io::SeekFrom::End(0)) {
                Ok(p) => p,
                Err(e) => return Some(Err(e.into())),
            };
            let remaining = end.saturating_sub(cur) as usize;
            self.done = true;
            if remaining > 0 {
                vlog!(
                    "diff::DiffFragmentIter::next_raw: emitting DELETED fragment len={}",
                    remaining
                );
                return Some(Ok(DiffFragment::DELETED { len: remaining }));
            }
            return None;
        }

        if old_eof {
            vlog!("diff::DiffFragmentIter::next_raw: old file EOF, processing remaining new file");
            // Emit at most max_fragment_size bytes per ADDED fragment so the
            // iterator never materialises the whole new file at once.
            let mut buf = Vec::new();
            match self
                .new
                .by_ref()
                .take(self.max_fragment_size as u64)
                .read_to_end(&mut buf)
            {
                Ok(0) => {
                    vlog!("diff::DiffFragmentIter::next_raw: no more data in new file");
                    self.done = true;
                    return None;
                }
                Ok(_) => {
                    vlog!(
                        "diff::DiffFragmentIter::next_raw: emitting ADDED fragment len={}",
                        buf.len()
                    );
                    return Some(Ok(DiffFragment::ADDED { body: buf }));
                }
                Err(e) => return Some(Err(e.into())),
            }
        }

        // Try convergence first
        match collect_convergence(&mut self.old, &mut self.new, self.window) {
            Ok(converged) if converged > 0 => {
                vlog!(
                    "diff::DiffFragmentIter::next_raw: found convergence len={}",
                    converged
                );
                return Some(Ok(DiffFragment::UNCHANGED { len: converged }));
            }
            Err(e) => return Some(Err(e)),
            _ => {}
        }

        // Then divergence
        match collect_divergence(
            &mut self.old,
            &mut self.new,
            self.window,
            self.max_fragment_size,
        ) {
            Ok((deleted, added)) => {
                vlog!(
                    "diff::DiffFragmentIter::next_raw: found divergence deleted={}, added={}",
                    deleted,
                    added.len()
                );
                // Queue all non-empty fragments
                if deleted > 0 {
                    self.queue.push(DiffFragment::DELETED { len: deleted });
                }
                if added.len() > 0 {
                    self.queue.push(DiffFragment::ADDED { body: added });
                }

                // Return first from queue
                if !self.queue.is_empty() {
                    vlog!("diff::DiffFragmentIter::next_raw: returning first queued divergence fragment");
                    return Some(Ok(self.queue.remove(0)));
                }

                // Nothing happened, try again
                vlog!("diff::DiffFragmentIter::next_raw: no fragments generated, retrying");
                self.next_raw()
            }
            Err(e) => Some(Err(e)),
        }
    }
}

impl<R: Read + Seek> Iterator for DiffFragmentIter<R> {
    type Item = Result<DiffFragment, Box<dyn std::error::Error>>;

    fn next(&mut self) -> Option<Self::Item> {
        // Get next raw fragment
        let next_frag = match self.next_raw() {
            Some(Ok(f)) => f,
            other => {
                // If we have a pending fragment, return it first
                if let Some(p) = self.pending.take() {
                    vlog!(
                        "diff::DiffFragmentIter::next: returning pending fragment before error/end"
                    );
                    // Put other back somehow? Actually if it's None or Err,
                    // we should return pending then handle other on next call
                    // This is tricky - for now just return pending
                    return Some(Ok(p));
                }
                return other;
            }
        };

        // Try to consolidate with pending
        match (&mut self.pending, &next_frag) {
            (Some(DiffFragment::UNCHANGED { len: prev_len }), DiffFragment::UNCHANGED { len }) => {
                vlog!(
                    "diff::DiffFragmentIter::next: consolidating UNCHANGED fragments: {} + {}",
                    *prev_len,
                    len
                );
                *prev_len += len;
                self.next() // Recurse to get more or return pending
            }
            (Some(DiffFragment::DELETED { len: prev_len }), DiffFragment::DELETED { len }) => {
                vlog!(
                    "diff::DiffFragmentIter::next: consolidating DELETED fragments: {} + {}",
                    *prev_len,
                    len
                );
                *prev_len += len;
                self.next()
            }
            (Some(DiffFragment::ADDED { .. }), DiffFragment::ADDED { .. }) => {
                vlog!("diff::DiffFragmentIter::next: NOT consolidating ADDED fragments (bounded by max_fragment_size)");
                // Do NOT consolidate ADDED fragments – bodies are already bounded
                // by max_fragment_size; merging them would re-materialise an
                // arbitrarily large amount of file content in RAM.
                let result = self.pending.take();
                self.pending = Some(next_frag);
                result.map(Ok)
            }
            (None, _) => {
                vlog!("diff::DiffFragmentIter::next: storing first fragment as pending");
                // No pending, store this one and get next to check for consolidation
                self.pending = Some(next_frag);
                self.next()
            }
            (Some(_), _) => {
                vlog!("diff::DiffFragmentIter::next: different fragment types, returning pending");
                // Different types - return pending, store new
                let result = self.pending.take();
                self.pending = Some(next_frag);
                result.map(Ok)
            }
        }
    }
}

pub fn build_diff_fragments<R: Read + Seek>(
    old: R,
    new: R,
    window: usize,
    max_fragment_size: usize,
) -> DiffFragmentIter<R> {
    vlog!(
        "diff::build_diff_fragments: creating iterator with window={}, max_fragment_size={}",
        window,
        max_fragment_size
    );
    DiffFragmentIter::new(old, new, window, max_fragment_size)
}

/// Apply a sequence of `DiffFragment` values to `old`, writing the resulting
/// bytes into `out`.
///
/// Behavior:
/// - `UNCHANGED { len }`  — copy `len` bytes from `old` to `out`.
/// - `DELETED { len }`    — consume (skip) `len` bytes from `old`.
/// - `ADDED { body }`     — write `body` into `out`.
///
/// Returns an error if fragments attempt to read past the end of `old` or
/// any underlying I/O error occurs.
pub fn apply_fragments<R: Read, W: io::Write, I: Iterator<Item = DiffFragment>>(
    old: &mut R,
    fragments: I,
    out: &mut W,
) -> Result<(), Box<dyn std::error::Error>> {
    vlog!("diff::apply_fragments: starting fragment application");
    let mut fragment_count = 0;
    for frag in fragments {
        fragment_count += 1;
        match frag {
            DiffFragment::UNCHANGED { len } => {
                vlog!(
                    "diff::apply_fragments: applying UNCHANGED fragment len={}",
                    len
                );
                let mut remaining = len;
                let mut buf = [0u8; 8 * 1024];
                while remaining > 0 {
                    let to_read = std::cmp::min(remaining, buf.len());
                    let n = old.read(&mut buf[..to_read])?;
                    if n == 0 {
                        return Err("unexpected EOF while applying UNCHANGED fragment".into());
                    }
                    out.write_all(&buf[..n])?;
                    remaining -= n;
                }
            }
            DiffFragment::DELETED { len } => {
                vlog!(
                    "diff::apply_fragments: applying DELETED fragment len={}",
                    len
                );
                // Consume (and discard) `len` bytes from `old`.
                let mut remaining = len;
                let mut buf = [0u8; 8 * 1024];
                while remaining > 0 {
                    let to_read = std::cmp::min(remaining, buf.len());
                    let n = old.read(&mut buf[..to_read])?;
                    if n == 0 {
                        return Err("unexpected EOF while applying DELETED fragment".into());
                    }
                    remaining -= n;
                }
            }
            DiffFragment::ADDED { body } => {
                vlog!(
                    "diff::apply_fragments: applying ADDED fragment len={}",
                    body.len()
                );
                out.write_all(&body)?;
            }
        }
    }
    vlog!(
        "diff::apply_fragments: completed applying {} fragments",
        fragment_count
    );

    Ok(())
}

/// Convenience wrapper for applying an iterator that yields
/// `Result<DiffFragment, Box<dyn Error>>` (the same type produced by
/// `build_diff_fragments`). Errors from the fragment iterator are
/// propagated immediately.
pub fn apply_fragments_result_iter<
    R: Read,
    W: io::Write,
    I: Iterator<Item = Result<DiffFragment, Box<dyn std::error::Error>>>,
>(
    old: &mut R,
    fragments: I,
    out: &mut W,
) -> Result<(), Box<dyn std::error::Error>> {
    vlog!("diff::apply_fragments_result_iter: starting fragment application from result iterator");
    let mut fragment_count = 0;
    for frag_res in fragments {
        fragment_count += 1;
        let frag = frag_res?;
        apply_fragments(old, std::iter::once(frag), out)?;
    }
    vlog!(
        "diff::apply_fragments_result_iter: completed applying {} fragments",
        fragment_count
    );
    Ok(())
}
