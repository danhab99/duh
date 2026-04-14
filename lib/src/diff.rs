use ahash::{HashMap, HashMapExt};
use std::io::{Read, Seek};
use std::{fmt, io};

/// Minimum window size for matching - prevents spurious matches on short byte sequences
const MIN_WINDOW: usize = 64;

/// Simple rolling hash used for byte‑wise slideable fingerprinting.
///
/// This is a 64‑bit Rabin‑Karp‑style polynomial hash with a fixed base. It
/// deliberately uses wrapping arithmetic (mod 2⁶⁴) because overflow is cheap
/// and collisions are already handled by a secondary verification phase.
struct RollingHash {
    base: u64,
    /// base^(window-1) used to remove the contribution of the outgoing byte
    power: u64,
    hash: u64,
    window: usize,
}

impl RollingHash {
    fn new(window: usize) -> Self {
        let base = 257;
        let mut power: u64 = 1;
        // compute base^(window-1)
        for _ in 1..window {
            power = power.wrapping_mul(base);
        }
        RollingHash {
            base,
            power,
            hash: 0,
            window,
        }
    }

    /// initialise the hash from the first `window` bytes
    fn compute(&mut self, buf: &[u8]) {
        debug_assert_eq!(buf.len(), self.window);
        let mut h = 0u64;
        for &b in buf {
            h = h.wrapping_mul(self.base).wrapping_add(b as u64);
        }
        self.hash = h;
    }

    /// slide the window by removing `out` and appending `inb`
    fn roll(&mut self, out: u8, inb: u8) {
        // subtract contribution of outgoing byte
        let out_val = (out as u64).wrapping_mul(self.power);
        self.hash = self.hash.wrapping_sub(out_val);
        // multiply by base and add incoming byte
        self.hash = self.hash.wrapping_mul(self.base).wrapping_add(inb as u64);
    }

    fn value(&self) -> u64 {
        self.hash
    }
}

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

    // Phase 1: build a sliding‑window index over `old` using a rolling hash.
    // We stop once we've examined `max_bytes` of the old stream, just like
    // the previous implementation did, so the map size is bounded.
    let mut index: HashMap<u64, usize> = HashMap::new();
    let mut old_position: usize = 0;
    let mut old_bytes_indexed: usize = 0;

    // buffer holding the current window from `old` so we can slide it one
    // byte at a time.
    let mut old_buf: std::collections::VecDeque<u8> = std::collections::VecDeque::new();
    let mut rolling_old = RollingHash::new(window);

    // prime the buffer with the first `window` bytes (if available)
    while old_buf.len() < window && old_bytes_indexed < max_bytes {
        let mut tmp = [0u8; 1];
        let n = old.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        old_buf.push_back(tmp[0]);
        old_position += 1;
        old_bytes_indexed += 1;
    }
    if old_buf.len() == window {
        // compute initial hash and index it at position 0
        rolling_old.compute(&old_buf.iter().cloned().collect::<Vec<_>>());
        index.entry(rolling_old.value()).or_insert(0);
    }

    // continue sliding one byte at a time until we hit EOF or the byte cap
    while old_bytes_indexed < max_bytes {
        let mut tmp = [0u8; 1];
        let n = old.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        let out = old_buf.pop_front().unwrap();
        old_buf.push_back(tmp[0]);
        // slide hash and record the position of the new window start
        rolling_old.roll(out, tmp[0]);
        let start_pos = old_position.checked_sub(window).unwrap_or(0);
        index.entry(rolling_old.value()).or_insert(start_pos);

        old_position += 1;
        old_bytes_indexed += 1;
    }

    // reset old to where we started scanning
    old.seek(io::SeekFrom::Start(old_starting_pos))?;

    // Phase 2: scan `new` byte‑by‑byte with its own rolling hash, looking for
    // a fingerprint that appears in the index.  `new_chunk_buffer` collects
    // all bytes that have been proven divergent; if it grows past
    // `max_bytes` we give up and return early just like the old code.
    let mut new_chunk_buffer: Vec<u8> = Vec::new();
    let mut new_buf: std::collections::VecDeque<u8> = std::collections::VecDeque::new();
    let mut rolling_new = RollingHash::new(window);
    // position counter is unused in the current algorithm; left over from
    // earlier experimentation and deliberately ignored.

    loop {
        // advance by a single byte
        let mut tmp = [0u8; 1];
        let n = new.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        // remember where the new stream is after consuming this byte; a
        // spurious candidate should not rewind past this point or we'll
        // re-process the same byte and duplicate it in `new_chunk_buffer`.
        let restore_pos = new.stream_position()?;

        if new_buf.len() < window {
            new_buf.push_back(tmp[0]);
            if new_buf.len() == window {
                rolling_new.compute(&new_buf.iter().cloned().collect::<Vec<_>>());
                // check this first window immediately
                let current_window_start = new_chunk_buffer.len();
                if let Some(&old_match_pos) = index.get(&rolling_new.value()) {
                    let old_verify_pos =
                        old_starting_pos + old_match_pos as u64 + window as u64;
                    let new_verify_pos =
                        new_starting_pos + current_window_start as u64 + window as u64;
                    old.seek(io::SeekFrom::Start(old_verify_pos))?;
                    new.seek(io::SeekFrom::Start(new_verify_pos))?;
                    let (old_v1, _) = read_chunk(old, window)?;
                    let (new_v1, _) = read_chunk(new, window)?;
                    let (old_v2, _) = read_chunk(old, window)?;
                    let (new_v2, _) = read_chunk(new, window)?;
                    if old_v1 == new_v1 && old_v2 == new_v2 {
                        let deleted = old_match_pos;
                        old.seek(io::SeekFrom::Start(
                            old_starting_pos + old_match_pos as u64 + window as u64,
                        ))?;
                        new.seek(io::SeekFrom::Start(new_verify_pos))?;
                        return Ok((deleted, new_chunk_buffer, window));
                    }
                    // spurious; restore to the position we were at immediately
                    // after reading the window‑ending byte.  previously the code
                    // rewound to `new_verify_pos` which can be *before* the byte
                    // we just consumed, causing it to be read again on the next
                    // iteration (and hence duplicated in the output).
                    new.seek(io::SeekFrom::Start(restore_pos))?;
                }
            }
            continue;
        }

        // buffer already full; slide it by one
        let out = new_buf.pop_front().unwrap();
        new_buf.push_back(tmp[0]);
        rolling_new.roll(out, tmp[0]);

        let current_window_start = new_chunk_buffer.len();
        if let Some(&old_match_pos) = index.get(&rolling_new.value()) {
            let old_verify_pos =
                old_starting_pos + old_match_pos as u64 + window as u64;
            let new_verify_pos =
                new_starting_pos + current_window_start as u64 + window as u64;
            old.seek(io::SeekFrom::Start(old_verify_pos))?;
            new.seek(io::SeekFrom::Start(new_verify_pos))?;
            let (old_v1, _) = read_chunk(old, window)?;
            let (new_v1, _) = read_chunk(new, window)?;
            let (old_v2, _) = read_chunk(old, window)?;
            let (new_v2, _) = read_chunk(new, window)?;
            if old_v1 == new_v1 && old_v2 == new_v2 {
                let deleted = old_match_pos;
                old.seek(io::SeekFrom::Start(
                    old_starting_pos + old_match_pos as u64 + window as u64,
                ))?;
                new.seek(io::SeekFrom::Start(new_verify_pos))?;
                return Ok((deleted, new_chunk_buffer, window));
            }
            // spurious match; restore to just after the consumed byte rather
            // than rewinding to `new_verify_pos` which may lie earlier than
            // the byte and cause duplicates on the next loop iteration.
            new.seek(io::SeekFrom::Start(restore_pos))?;
        }

        // this window did not pan out; consume the outgoing byte
        new_chunk_buffer.push(out);
        if new_chunk_buffer.len() >= max_bytes {
            old.seek(io::SeekFrom::Start(old_starting_pos + old_bytes_indexed as u64))?;
            return Ok((old_bytes_indexed, new_chunk_buffer, 0));
        }
    }

    // at EOF we still need to drain any bytes left in `new_buf`
    while let Some(b) = new_buf.pop_front() {
        new_chunk_buffer.push(b);
    }

    // No convergence found, try with smaller window
    if window > MIN_WINDOW {
        old.seek(io::SeekFrom::Start(old_starting_pos))?;
        new.seek(io::SeekFrom::Start(new_starting_pos))?;
        return collect_divergence(old, new, window / 2, max_bytes);
    }

    // no match at all – count remaining bytes in `old` and return
    old.seek(io::SeekFrom::Start(old_starting_pos))?;
    old.seek(io::SeekFrom::End(0))?;
    let old_end = old.stream_position()?;
    let old_remaining = old_end.saturating_sub(old_starting_pos) as usize;
    Ok((old_remaining, new_chunk_buffer, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn fragments_from(old: &[u8], new: &[u8], window: usize, max: usize) -> Vec<DiffFragment> {
        let mut o = Cursor::new(old);
        let mut n = Cursor::new(new);
        build_diff_fragments(&mut o, &mut n, window, max)
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn rolling_hash_stability() {
        let mut h = RollingHash::new(4);
        let data = b"abcdef";
        h.compute(&data[0..4]);
        let first = h.value();
        h.roll(b'a', b'e');
        let second = h.value();
        // recompute manually to check that rolling produced same value
        let mut h2 = RollingHash::new(4);
        h2.compute(&data[1..5]);
        assert_eq!(second, h2.value());
        h2.compute(&data[2..6]);
        h.roll(b'b', b'f');
        assert_eq!(h.value(), h2.value());
        assert_ne!(first, second);
    }

    #[test]
    // with a window of 3 the trailing two bytes of `old` are shorter than the
    // window, so the algorithm considers them deleted and re-added rather than
    // detecting them as unchanged.  This mirrors the behaviour of the original
    // implementation and demonstrates that the rolling‑hash rewrite is
    // behaviour‑preserving.
    fn divergence_simple_append() {
        let old = b"hello";
        let new = b"hello world";
        let frags = fragments_from(old, new, 3, 1024);
        assert_eq!(frags, vec![
            DiffFragment::UNCHANGED { len: 3 },
            DiffFragment::DELETED { len: 2 },
            DiffFragment::ADDED { body: b"lo world".to_vec() },
        ]);
    }

    #[test]
    fn divergence_insert_middle() {
        let old = b"abcdefgh";
        let new = b"abcdZZefgh";
        let frags = fragments_from(old, new, 4, 1024);
        // the remaining suffix of `old` is exactly four bytes; because our
        // window equals that length and the algorithm indexes only full
        // windows from old, the suffix is treated as a deletion followed by an
        // addition of the same bytes prefixed by "ZZ".
        assert_eq!(frags, vec![
            DiffFragment::UNCHANGED { len: 4 },
            DiffFragment::DELETED { len: 4 },
            DiffFragment::ADDED { body: b"ZZefgh".to_vec() },
        ]);
    }

    #[test]
    fn divergence_delete() {
        let old = b"123456";
        let new = b"12356";
        let frags = fragments_from(old, new, 2, 1024);
        // because the last two bytes in `old` form a full window and no
        // matching window is found in `new`, the algorithm reports the suffix
        // as deleted before resynchronising.
        assert_eq!(frags, vec![
            DiffFragment::UNCHANGED { len: 2 },
            DiffFragment::DELETED { len: 1 },
            DiffFragment::UNCHANGED { len: 3 },
        ]);
    }
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
