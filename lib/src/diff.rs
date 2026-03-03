use ahash::{HashMap, HashMapExt};
use std::io::{Read, Seek};
use std::{fmt, io};

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

pub fn collect_divergence<R: Read + Seek>(
    old: &mut R,
    new: &mut R,
    window: usize,
    max_bytes: usize,
) -> Result<(usize, Vec<u8>, usize), Box<dyn std::error::Error>> {
    // Returns: (deleted_bytes, added_bytes, matched_bytes)
    let old_starting_pos = old.stream_position()?;
    let new_starting_pos = new.stream_position()?;

    // Phase 1: Build hash index from old, capped at max_bytes so the HashMap
    // never exceeds ~(max_bytes / window) entries regardless of file size.
    // Keys are 64-bit truncations of the blake3 hash; the double-verify step
    // in phase 2 catches the negligible collision rate.
    let mut index: HashMap<u64, usize> = HashMap::new();
    let mut old_position: usize = 0;
    let mut old_bytes_indexed: usize = 0;
    
    loop {
        if old_bytes_indexed >= max_bytes {
            break;
        }
        let (old_chunk, old_eof) = read_chunk(old, window)?;
        if old_chunk.is_empty() {
            break;
        }
        
        let old_fp = u64::from_le_bytes(
            blake3::hash(&old_chunk).as_bytes()[..8].try_into().unwrap()
        );
        // Keep only the first occurrence so repeated-content files (e.g. all
        // the same byte) always match at the earliest possible old position,
        // keeping the streams aligned after a match is confirmed.
        index.entry(old_fp).or_insert(old_position);
        
        old_position += old_chunk.len();
        old_bytes_indexed += old_chunk.len();
        
        if old_eof {
            break;
        }
    }
    
    // Reset old to starting position
    old.seek(io::SeekFrom::Start(old_starting_pos))?;
    
    // Phase 2: Slide through new file looking for matches.
    // Cap new_chunk_buffer at max_bytes so no single divergent section
    // materialises more than max_bytes of new-file content at once.
    let mut new_chunk_buffer: Vec<u8> = Vec::new();
    
    loop {
        // Early-return once we have a full max_bytes chunk of new without a
        // match.  Advance old by old_bytes_indexed so the caller emits a
        // DELETED for those bytes and old does not stall at the same position.
        if new_chunk_buffer.len() >= max_bytes {
            old.seek(io::SeekFrom::Start(old_starting_pos + old_bytes_indexed as u64))?;
            return Ok((old_bytes_indexed, new_chunk_buffer, 0));
        }
        let (new_chunk, new_eof) = read_chunk(new, window)?;
        if new_chunk.is_empty() {
            break;
        }
        
        let new_fp = u64::from_le_bytes(
            blake3::hash(&new_chunk).as_bytes()[..8].try_into().unwrap()
        );
        
        if let Some(&old_match_pos) = index.get(&new_fp) {
            // Found potential match - verify with next 2 windows
            let old_verify_pos = old_starting_pos + old_match_pos as u64 + window as u64;
            let new_verify_pos = new_starting_pos + new_chunk_buffer.len() as u64 + window as u64;
            
            old.seek(io::SeekFrom::Start(old_verify_pos))?;
            new.seek(io::SeekFrom::Start(new_verify_pos))?;
            
            let (old_verify1, _) = read_chunk(old, window)?;
            let (new_verify1, _) = read_chunk(new, window)?;
            let (old_verify2, _) = read_chunk(old, window)?;
            let (new_verify2, _) = read_chunk(new, window)?;
            
            if old_verify1 == new_verify1 && old_verify2 == new_verify2 {
                // True match confirmed
                let deleted = old_match_pos;
                
                // Position streams after the matched window
                old.seek(io::SeekFrom::Start(old_starting_pos + old_match_pos as u64 + window as u64))?;
                new.seek(io::SeekFrom::Start(new_verify_pos))?;
                
                return Ok((deleted, new_chunk_buffer, window));
            }
            
            // Spurious match - restore new position and continue
            new.seek(io::SeekFrom::Start(new_starting_pos + new_chunk_buffer.len() as u64 + window as u64))?;
        }
        
        new_chunk_buffer.extend_from_slice(&new_chunk);
        
        if new_eof {
            break;
        }
    }

    // No convergence found, try with smaller window
    if window > MIN_WINDOW {
        old.seek(io::SeekFrom::Start(old_starting_pos))?;
        new.seek(io::SeekFrom::Start(new_starting_pos))?;
        return collect_divergence(old, new, window / 2, max_bytes);
    }
    
    // No match at all – `new_chunk_buffer` already holds the complete new
    // remainder from the scan above, so there is no need to seek back and
    // call read_to_end again.  For the old side we only need the byte count
    // (DELETED carries no body), which we get by seeking to EOF.
    old.seek(io::SeekFrom::Start(old_starting_pos))?;
    old.seek(io::SeekFrom::End(0))?;
    let old_end = old.stream_position()?;
    let old_remaining = old_end.saturating_sub(old_starting_pos) as usize;

    Ok((old_remaining, new_chunk_buffer, 0))
}

fn collect_convergence<R: Read + Seek>(
    old: &mut R,
    new: &mut R,
    window: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
    if window < 1 {
        return Err("window is too small".into());
    }

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
            // Found divergence, rewind to before this chunk
            old.seek(io::SeekFrom::Start(old_starting_pos + unchanged_len as u64))?;
            new.seek(io::SeekFrom::Start(new_starting_pos + unchanged_len as u64))?;
            break;
        }

        unchanged_len += old_buf.len();

        if hit_eof {
            return Ok(unchanged_len);
        }
    }

    if unchanged_len == 0 {
        // Files diverge immediately at this position - that's fine, return 0
        old.seek(io::SeekFrom::Start(old_starting_pos))?;
        new.seek(io::SeekFrom::Start(new_starting_pos))?;
        return Ok(0);
    }

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
            return Some(Ok(self.queue.remove(0)));
        }
        
        if self.done {
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
            self.done = true;
            return None;
        }
        
        // If only one is EOF, handle remaining
        if new_eof {
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
                return Some(Ok(DiffFragment::DELETED { len: remaining }));
            }
            return None;
        }
        
        if old_eof {
            // Emit at most max_fragment_size bytes per ADDED fragment so the
            // iterator never materialises the whole new file at once.
            let mut buf = Vec::new();
            match self.new.by_ref().take(self.max_fragment_size as u64).read_to_end(&mut buf) {
                Ok(0) => {
                    self.done = true;
                    return None;
                }
                Ok(_) => return Some(Ok(DiffFragment::ADDED { body: buf })),
                Err(e) => return Some(Err(e.into())),
            }
        }
        
        // Try convergence first
        match collect_convergence(&mut self.old, &mut self.new, self.window) {
            Ok(converged) if converged > 0 => {
                return Some(Ok(DiffFragment::UNCHANGED { len: converged }));
            }
            Err(e) => return Some(Err(e)),
            _ => {}
        }
        
        // Then divergence
        match collect_divergence(&mut self.old, &mut self.new, self.window, self.max_fragment_size) {
            Ok((deleted, added, matched)) => {
                // Queue all non-empty fragments
                if deleted > 0 {
                    self.queue.push(DiffFragment::DELETED { len: deleted });
                }
                if added.len() > 0 {
                    self.queue.push(DiffFragment::ADDED { body: added });
                }
                if matched > 0 {
                    self.queue.push(DiffFragment::UNCHANGED { len: matched });
                }
                
                // Return first from queue
                if !self.queue.is_empty() {
                    return Some(Ok(self.queue.remove(0)));
                }
                
                // Nothing happened, try again
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
                *prev_len += len;
                self.next() // Recurse to get more or return pending
            }
            (Some(DiffFragment::DELETED { len: prev_len }), DiffFragment::DELETED { len }) => {
                *prev_len += len;
                self.next()
            }
            (Some(DiffFragment::ADDED { .. }), DiffFragment::ADDED { .. }) => {
                // Do NOT consolidate ADDED fragments – bodies are already bounded
                // by max_fragment_size; merging them would re-materialise an
                // arbitrarily large amount of file content in RAM.
                let result = self.pending.take();
                self.pending = Some(next_frag);
                result.map(Ok)
            }
            (None, _) => {
                // No pending, store this one and get next to check for consolidation
                self.pending = Some(next_frag);
                self.next()
            }
            (Some(_), _) => {
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
    for frag in fragments {
        match frag {
            DiffFragment::UNCHANGED { len } => {
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
                out.write_all(&body)?;
            }
        }
    }

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
    for frag_res in fragments {
        let frag = frag_res?;
        apply_fragments(old, std::iter::once(frag), out)?;
    }
    Ok(())
}
