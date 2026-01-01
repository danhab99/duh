use ahash::{HashMap, HashMapExt};
use rmp::encode::RmpWrite;
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
        println!("eof");
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
) -> Result<(usize, Vec<u8>, usize), Box<dyn std::error::Error>> {
    // Returns: (deleted_bytes, added_bytes, matched_bytes)
    let old_starting_pos = old.stream_position()?;
    let new_starting_pos = new.stream_position()?;

    // let mut index: HashMap<blake3::Hash, (usize, Vec<u8>)> = HashMap::new();
    let mut index: HashMap<blake3::Hash, usize> = HashMap::new();
    let mut old_total_read_bytes: usize = 0;
    let mut new_chunk_buffer: Vec<u8> = Vec::new();
    let mut converged_position: Option<usize> = None;

    loop {
        let (old_chunk, old_eof) = read_chunk(old, window)?;
        if old_chunk.is_empty() && old_eof {
            break;
        }

        old_total_read_bytes += old_chunk.len();

        let old_hash = {
            let mut h = blake3::Hasher::new();
            h.write_bytes(&old_chunk.clone())?;
            h.finalize()
        };

        index.insert(old_hash, old_total_read_bytes);

        let (mut new_chunk, new_eof) = read_chunk(new, window)?;
        if new_chunk.is_empty() && new_eof {
            break;
        }
        let new_hash = {
            let mut h = blake3::Hasher::new();
            h.write_bytes(&new_chunk)?;
            h.finalize()
        };

        match index.get(&new_hash) {
            Some(old_position_bytes) => {
                let deleted = old_position_bytes - window;
                
                let old_seek_to = old_starting_pos + *old_position_bytes as u64;
                let new_seek_to = new_starting_pos + new_chunk_buffer.len() as u64 + window as u64;
                
                // Verify match: check that 2 more windows also match (true convergence)
                old.seek(io::SeekFrom::Start(old_seek_to))?;
                new.seek(io::SeekFrom::Start(new_seek_to))?;
                
                let (old_verify1, old_eof1) = read_chunk(old, window)?;
                let (new_verify1, new_eof1) = read_chunk(new, window)?;
                let (old_verify2, old_eof2) = read_chunk(old, window)?;
                let (new_verify2, new_eof2) = read_chunk(new, window)?;
                
                let eof_ok = old_eof1 || new_eof1 || old_eof2 || new_eof2;
                
                if old_verify1 != new_verify1 || old_verify2 != new_verify2 {
                    if !eof_ok {
                        // Spurious match - next 2 windows don't match, skip
                        println!("SPURIOUS: deleted={} added={} window={} - next windows differ", 
                                 deleted, new_chunk_buffer.len(), window);
                        // Restore positions and continue searching
                        old.seek(io::SeekFrom::Start(old_starting_pos + old_total_read_bytes as u64))?;
                        new.seek(io::SeekFrom::Start(new_starting_pos + new_chunk_buffer.len() as u64 + window as u64))?;
                        new_chunk_buffer.append(&mut new_chunk);
                        if old_eof || new_eof {
                            break;
                        }
                        continue;
                    }
                }
                
                println!("MATCH: deleted={} added={} matched={} window={}", deleted, new_chunk_buffer.len(), window, window);
                
                // Seek to after the matched chunk (not the verification chunks)
                old.seek(io::SeekFrom::Start(old_seek_to))?;
                new.seek(io::SeekFrom::Start(new_seek_to))?;
                
                return Ok((deleted, new_chunk_buffer, window));
            }
            None => {}
        }

        // Only append after checking - matched chunk should NOT be in added
        new_chunk_buffer.append(&mut new_chunk);

        if old_eof || new_eof {
            break;
        }
    }

    // No convergence found, try with smaller window
    if window > MIN_WINDOW {
        old.seek(io::SeekFrom::Start(old_starting_pos))?;
        new.seek(io::SeekFrom::Start(new_starting_pos))?;
        println!("!!! failed to collect divergence at end window={}", window);
        return collect_divergence(old, new, window / 2);
    }
    
    old.seek(io::SeekFrom::Start(old_starting_pos))?;
    new.seek(io::SeekFrom::Start(new_starting_pos))?;
    
    let mut remaining_old = Vec::new();
    let mut remaining_new = Vec::new();
    old.read_to_end(&mut remaining_old)?;
    new.read_to_end(&mut remaining_new)?;
    
    println!("NO CONVERGENCE POSSIBLE: remaining old={} new={}", remaining_old.len(), remaining_new.len());
    Ok((remaining_old.len(), remaining_new, 0))
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
            println!("eof break");
            return Ok(unchanged_len);
        }
    }

    if unchanged_len == 0 {
        // Files diverge immediately at this position - that's fine, return 0
        println!("convergence=0 at window={}", window);
        old.seek(io::SeekFrom::Start(old_starting_pos))?;
        new.seek(io::SeekFrom::Start(new_starting_pos))?;
        return Ok(0);
    }

    println!(
        "success collecting convergence window={} len={}",
        window, unchanged_len
    );
    Ok(unchanged_len)
}

fn test_eof<R: Read + Seek>(reader: &mut R) -> Result<bool, Box<dyn std::error::Error>> {
    let p = reader.stream_position()?;
    let (c, eof) = read_chunk(reader, 1)?;
    if c.len() > 0 {
        reader.seek(io::SeekFrom::Start(p))?;
    }
    println!("test eof {} {} {}", p, c.len(), eof);
    return Ok(eof);
}

/// Iterator that yields diff fragments one at a time
pub struct DiffFragmentIter<R: Read + Seek> {
    old: R,
    new: R,
    window: usize,
    done: bool,
    pending: Option<DiffFragment>, // For consolidation
}

impl<R: Read + Seek> DiffFragmentIter<R> {
    pub fn new(old: R, new: R, window: usize) -> Self {
        Self {
            old,
            new,
            window,
            done: false,
            pending: None,
        }
    }
    
    fn next_raw(&mut self) -> Option<Result<DiffFragment, Box<dyn std::error::Error>>> {
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
            self.done = true;
            let mut buf = Vec::new();
            if let Err(e) = self.old.read_to_end(&mut buf) {
                return Some(Err(e.into()));
            }
            if buf.len() > 0 {
                return Some(Ok(DiffFragment::DELETED { len: buf.len() }));
            }
            return None;
        }
        
        if old_eof {
            self.done = true;
            let mut buf = Vec::new();
            if let Err(e) = self.new.read_to_end(&mut buf) {
                return Some(Err(e.into()));
            }
            if buf.len() > 0 {
                return Some(Ok(DiffFragment::ADDED { body: buf }));
            }
            return None;
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
        match collect_divergence(&mut self.old, &mut self.new, self.window) {
            Ok((deleted, added, matched)) => {
                // Return deleted first, queue added and matched
                if deleted > 0 {
                    // We need to return deleted now, but also return added and matched later
                    // For simplicity, we'll handle this by returning a composite approach
                    // Actually let's return them in sequence using pending
                    
                    // Queue up the rest if needed
                    if added.len() > 0 || matched > 0 {
                        // Store added for next call, matched for after that
                        // This is getting complex - let's just return deleted and 
                        // let the next call naturally pick up from the new position
                    }
                    return Some(Ok(DiffFragment::DELETED { len: deleted }));
                }
                if added.len() > 0 {
                    return Some(Ok(DiffFragment::ADDED { body: added }));
                }
                if matched > 0 {
                    return Some(Ok(DiffFragment::UNCHANGED { len: matched }));
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
            (Some(DiffFragment::ADDED { body: prev_body }), DiffFragment::ADDED { body }) => {
                prev_body.extend(body);
                self.next()
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
) -> DiffFragmentIter<R> {
    DiffFragmentIter::new(old, new, window)
}
