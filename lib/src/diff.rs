use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io::{Read, Write, Result, Seek, SeekFrom, Cursor};

#[derive()]
#[derive(PartialEq, Eq, Clone, Debug, serde::Deserialize, serde::Serialize, Copy)]
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

/// Represents a fragment type and provides a reader for its data
pub struct Fragment {
    pub kind: DiffFragment,
    pub len: usize,
    pub reader: Box<dyn Read>,
}

/// Streaming diff processor that computes fragments lazily
/// 
/// This struct maintains state and produces one fragment at a time,
/// allowing the caller to process each fragment without loading
/// the entire file into memory.
/// 
/// # Memory Usage
/// - Signature table from old file: ~block_size bytes per block
/// - Rolling hash window: block_size bytes
/// - ADDED accumulator: grows until match found, then yielded
/// - Total: O(old_file_blocks * block_size + accumulated_added_bytes)
/// 
/// # Example
/// ```
/// let mut streamer = DiffStreamer::new(old_file, new_file, 512)?;
/// while let Some(fragment) = streamer.next_fragment()? {
///     // fragment.reader can be read to get the fragment data
///     // Process immediately without buffering
/// }
/// ```
pub struct DiffStreamer<R: Read> {
    new_reader: R,
    block_size: usize,
    
    // Signature table built from old file
    signatures: HashMap<u64, Vec<BlockSignature>>,
    old_file_len: usize,
    
    // Rolling hash state for new file
    rolling_hash: RollingHash,
    
    // Accumulator for ADDED bytes (between matches)
    added_buffer: Vec<u8>,
    
    // Track used ranges in old file for DELETED detection
    used_ranges: Vec<(usize, usize)>,
    
    // Current position in new file
    new_pos: usize,
    
    // Pending fragment to yield (set when we find a match)
    pending: Option<Fragment>,
    
    // State
    eof: bool,
    done: bool,
}

impl<R: Read> DiffStreamer<R> {
    /// Create a new streaming diff processor
    /// 
    /// Streams old file once to build signature table, then streams new file.
    pub fn new<R1: Read>(mut old: R1, new: R, block_size: usize) -> Result<Self> {
        // Build signature table by streaming old file
        let mut signatures: HashMap<u64, Vec<BlockSignature>> = HashMap::new();
        let mut old_file_len = 0;
        let mut offset = 0;
        let mut buffer = vec![0u8; block_size];
        
        loop {
            let n = old.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            
            old_file_len += n;
            
            // Only create signature if we have a full block
            if n == block_size {
                let mut hasher = RollingHash::new(257, 1_000_000_007, block_size);
                for &byte in &buffer[..n] {
                    hasher.push(byte);
                }
                
                let hash = hasher.digest();
                signatures.entry(hash).or_insert_with(Vec::new).push(BlockSignature {
                    offset,
                    hash,
                    data: buffer[..n].to_vec(),
                });
            }
            
            offset += n;
        }
        
        Ok(Self {
            new_reader: new,
            block_size,
            signatures,
            old_file_len,
            rolling_hash: RollingHash::new(257, 1_000_000_007, block_size),
            added_buffer: Vec::new(),
            used_ranges: Vec::new(),
            new_pos: 0,
            pending: None,
            eof: false,
            done: false,
        })
    }
    
    /// Get the next fragment, returning None when diff is complete
    pub fn next_fragment(&mut self) -> Result<Option<Fragment>> {
        if self.done {
            return Ok(None);
        }
        
        // If we have a pending fragment, yield it first
        if let Some(frag) = self.pending.take() {
            return Ok(Some(frag));
        }
        
        // Stream through new file looking for matches
        loop {
            // Try to read one byte
            let mut byte = [0u8; 1];
            let n = self.new_reader.read(&mut byte)?;
            
            if n == 0 {
                // EOF reached
                self.eof = true;
                
                // Yield any remaining ADDED bytes
                if !self.added_buffer.is_empty() {
                    let data = std::mem::take(&mut self.added_buffer);
                    let len = data.len();
                    return Ok(Some(Fragment {
                        kind: DiffFragment::ADDED { body: vec![] },
                        len,
                        reader: Box::new(Cursor::new(data)),
                    }));
                }
                
                // Now yield DELETED fragments for unused parts of old file
                return self.next_deleted_fragment();
            }
            
            let byte = byte[0];
            self.rolling_hash.push(byte);
            self.new_pos += 1;
            
            // Check if we have a full window
            if self.rolling_hash.len() < self.block_size {
                self.added_buffer.push(byte);
                continue;
            }
            
            // Check for match
            let hash = self.rolling_hash.digest();
            if let Some(match_info) = self.find_match(hash) {
                // Found a match!
                
                // First, yield accumulated ADDED bytes (minus the matched block)
                let pre_match_len = self.added_buffer.len() - self.block_size;
                if pre_match_len > 0 {
                    let added_data = self.added_buffer[..pre_match_len].to_vec();
                    self.added_buffer.drain(..pre_match_len);
                    
                    let len = added_data.len();
                    self.pending = Some(Fragment {
                        kind: DiffFragment::UNCHANGED { len: match_info.len },
                        len: match_info.len,
                        reader: Box::new(std::io::empty()),
                    });
                    
                    return Ok(Some(Fragment {
                        kind: DiffFragment::ADDED { body: vec![] },
                        len,
                        reader: Box::new(Cursor::new(added_data)),
                    }));
                }
                
                // No accumulated bytes, yield the UNCHANGED directly
                self.added_buffer.clear();
                self.rolling_hash = RollingHash::new(257, 1_000_000_007, self.block_size);
                
                return Ok(Some(Fragment {
                    kind: DiffFragment::UNCHANGED { len: match_info.len },
                    len: match_info.len,
                    reader: Box::new(std::io::empty()),
                }));
            }
            
            // No match, keep accumulating
            self.added_buffer.push(byte);
        }
    }
    
    fn find_match(&mut self, hash: u64) -> Option<MatchInfo> {
        let candidates = self.signatures.get(&hash)?;
        
        let window = self.rolling_hash.current_window();
        
        for candidate in candidates {
            if candidate.data != window {
                continue;
            }
            
            // Check if already used
            let is_used = self.used_ranges.iter().any(|(start, end)| {
                candidate.offset < *end && candidate.offset + self.block_size > *start
            });
            
            if is_used {
                continue;
            }
            
            // Found valid match - extend it if possible
            let match_len = self.block_size; // TODO: extend by reading ahead
            
            self.used_ranges.push((candidate.offset, candidate.offset + match_len));
            
            return Some(MatchInfo {
                old_offset: candidate.offset,
                len: match_len,
            });
        }
        
        None
    }
    
    fn next_deleted_fragment(&mut self) -> Result<Option<Fragment>> {
        // Find next unused range in old file
        self.used_ranges.sort_by_key(|(start, _)| *start);
        
        let mut search_pos = 0;
        for (start, end) in &self.used_ranges {
            if search_pos < *start {
                // Found a gap
                let len = start - search_pos;
                self.used_ranges.insert(0, (search_pos, *start)); // Mark as processed
                return Ok(Some(Fragment {
                    kind: DiffFragment::DELETED { len },
                    len,
                    reader: Box::new(std::io::empty()),
                }));
            }
            search_pos = search_pos.max(*end);
        }
        
        // Check for trailing deleted bytes
        if search_pos < self.old_file_len {
            let len = self.old_file_len - search_pos;
            self.done = true;
            return Ok(Some(Fragment {
                kind: DiffFragment::DELETED { len },
                len,
                reader: Box::new(std::io::empty()),
            }));
        }
        
        self.done = true;
        Ok(None)
    }
}

#[derive(Debug)]
struct MatchInfo {
    old_offset: usize,
    len: usize,
}

fn build_signature_table(old_buffer: &[u8], block_size: usize) -> HashMap<u64, Vec<BlockSignature>> {
    let mut signatures: HashMap<u64, Vec<BlockSignature>> = HashMap::new();
    
    if old_buffer.len() >= block_size {
        for offset in 0..=(old_buffer.len() - block_size) {
            let block = &old_buffer[offset..offset + block_size];
            let mut hasher = RollingHash::new(257, 1_000_000_007, block_size);
            
            for &byte in block {
                hasher.push(byte);
            }
            
            let hash = hasher.digest();
            signatures.entry(hash).or_insert_with(Vec::new).push(BlockSignature {
                offset,
                hash,
                data: block.to_vec(),
            });
        }
    }
    
    signatures
}


// Simple Rabin-Karp rolling hash
struct RollingHash {
    window: VecDeque<u8>,
    hash: u64,
    base: u64,
    modulus: u64,
    pow_base: u64,
    capacity: usize,
}

impl RollingHash {
    fn new(base: u64, modulus: u64, capacity: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(capacity),
            hash: 0,
            base,
            modulus,
            pow_base: 1,
            capacity,
        }
    }

    fn push(&mut self, byte: u8) {
        // If at capacity, roll the window
        if self.window.len() >= self.capacity {
            self.pop();
        }
        
        self.window.push_back(byte);
        self.hash = (self.hash * self.base + byte as u64) % self.modulus;
        if self.window.len() > 1 {
            self.pow_base = (self.pow_base * self.base) % self.modulus;
        }
    }

    fn pop(&mut self) {
        if let Some(front) = self.window.pop_front() {
            self.hash = (self.hash + self.modulus - (front as u64 * self.pow_base) % self.modulus)
                % self.modulus;
            if self.window.len() > 0 {
                self.pow_base = (self.pow_base * mod_inverse(self.base, self.modulus)) % self.modulus;
            }
        }
    }

    fn len(&self) -> usize {
        self.window.len()
    }

    fn digest(&self) -> u64 {
        self.hash
    }

    fn current_window(&mut self) -> &[u8] {
        self.window.make_contiguous()
    }
}

// Simple modular inverse using extended Euclidean algorithm
fn mod_inverse(a: u64, m: u64) -> u64 {
    let (mut old_r, mut r) = (a as i64, m as i64);
    let (mut old_s, mut s) = (1i64, 0i64);
    
    while r != 0 {
        let quotient = old_r / r;
        let temp_r = old_r - quotient * r;
        old_r = r;
        r = temp_r;
        
        let temp_s = old_s - quotient * s;
        old_s = s;
        s = temp_s;
    }
    
    ((old_s % m as i64 + m as i64) % m as i64) as u64
}

#[derive(Clone, Debug)]
struct BlockSignature {
    offset: usize,
    hash: u64,
    data: Vec<u8>,
}

pub fn diff_streams<R1: Read, R2: Read>(
    old: R1,
    new: R2,
    block_size: usize,
) -> Result<Vec<DiffFragment>> {
    // Compatibility wrapper: use the streaming DiffStreamer but collect all results
    let mut streamer = DiffStreamer::new(old, new, block_size)?;
    let mut fragments = Vec::new();
    
    while let Some(fragment) = streamer.next_fragment()? {
        let frag = match fragment.kind {
            DiffFragment::ADDED { .. } => {
                let mut body = Vec::new();
                let mut reader = fragment.reader;
                reader.read_to_end(&mut body)?;
                DiffFragment::ADDED { body }
            }
            DiffFragment::UNCHANGED { .. } => {
                DiffFragment::UNCHANGED { len: fragment.len }
            }
            DiffFragment::DELETED { .. } => {
                DiffFragment::DELETED { len: fragment.len }
            }
        };
        fragments.push(frag);
    }
    
    Ok(merge_fragments(fragments))
}

fn merge_fragments(fragments: Vec<DiffFragment>) -> Vec<DiffFragment> {
    if fragments.is_empty() {
        return fragments;
    }

    let mut merged = Vec::new();
    let mut current = fragments[0].clone();

    for next in fragments.into_iter().skip(1) {
        match (&current, &next) {
            (DiffFragment::ADDED { body: body1 }, DiffFragment::ADDED { body: body2 }) => {
                let mut combined = body1.clone();
                combined.extend_from_slice(body2);
                current = DiffFragment::ADDED { body: combined };
            }
            (DiffFragment::UNCHANGED { len: len1 }, DiffFragment::UNCHANGED { len: len2 }) => {
                current = DiffFragment::UNCHANGED { len: len1 + len2 };
            }
            (DiffFragment::DELETED { len: len1 }, DiffFragment::DELETED { len: len2 }) => {
                current = DiffFragment::DELETED { len: len1 + len2 };
            }
            _ => {
                merged.push(current);
                current = next;
            }
        }
    }
    merged.push(current);

    merged
}

/// Apply a diff patch to a file stream to reconstruct the result
/// Streams output to avoid loading entire file in memory - critical for large files
pub fn apply_diff<R: Read + Seek, W: Write>(
    mut old: R,
    diff: &[DiffFragment],
    mut output: W,
) -> Result<()> {
    const BUFFER_SIZE: usize = 8 * 1024 * 1024; // 8MB buffer for chunked reads
    
    for fragment in diff {
        match fragment {
            DiffFragment::ADDED { body } => {
                output.write_all(body)?;
            }
            DiffFragment::UNCHANGED { len } => {
                // Stream copy in chunks to avoid loading everything
                let mut remaining = *len;
                let mut buffer = vec![0u8; BUFFER_SIZE.min(remaining)];
                
                while remaining > 0 {
                    let to_read = BUFFER_SIZE.min(remaining);
                    let bytes_read = old.read(&mut buffer[..to_read]).unwrap_or(0usize);
                    if bytes_read == 0usize {
                        break;
                    }
                    output.write_all(&buffer[..bytes_read])?;
                    remaining -= bytes_read;
                }
            }
            DiffFragment::DELETED { len } => {
                // Skip bytes by seeking forward
                old.seek(SeekFrom::Current(*len as i64))?;
            }
        }
    }
    
    Ok(())
}

/// Stateful patch composer that processes patches incrementally
/// This avoids loading all patch data into memory at once
pub struct PatchComposer {
    // Intermediate representation after first patch
    segments: Vec<IntermediateSegment>,
    // Accumulated result fragments
    result: Vec<DiffFragment>,
    // Current position in segments
    intermediate_pos: usize,
    segment_offset: usize,
}

#[derive(Debug, Clone)]
enum IntermediateSegment {
    FromOriginal { offset: usize, len: usize },
    Added { data: Vec<u8> },
}

impl PatchComposer {
    /// Create a new composer and process the first patch
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
            result: Vec::new(),
            intermediate_pos: 0,
            segment_offset: 0,
        }
    }
    
    /// Apply the first patch (A→B) to build intermediate representation
    pub fn apply_first_patch(&mut self, patch1: &[DiffFragment]) {
        let mut original_offset = 0;
        
        for fragment in patch1 {
            match fragment {
                DiffFragment::ADDED { body } => {
                    self.segments.push(IntermediateSegment::Added { data: body.clone() });
                }
                DiffFragment::UNCHANGED { len } => {
                    self.segments.push(IntermediateSegment::FromOriginal {
                        offset: original_offset,
                        len: *len,
                    });
                    original_offset += len;
                }
                DiffFragment::DELETED { len } => {
                    // Deleted bytes don't appear in intermediate
                    original_offset += len;
                }
            }
        }
    }
    
    /// Apply the second patch (B→C) to produce final composed patch (A→C)
    pub fn apply_second_patch(&mut self, patch2: &[DiffFragment]) {
        for fragment in patch2 {
            match fragment {
                DiffFragment::ADDED { body } => {
                    self.result.push(DiffFragment::ADDED { body: body.clone() });
                }
                DiffFragment::UNCHANGED { len } => {
                    self.process_unchanged(*len);
                }
                DiffFragment::DELETED { len } => {
                    self.process_deleted(*len);
                }
            }
        }
    }
    
    fn process_unchanged(&mut self, mut remaining: usize) {
        while remaining > 0 && self.intermediate_pos < self.segments.len() {
            match &self.segments[self.intermediate_pos] {
                IntermediateSegment::FromOriginal { offset: _, len: seg_len } => {
                    let available = seg_len - self.segment_offset;
                    let take = remaining.min(available);
                    
                    self.result.push(DiffFragment::UNCHANGED { len: take });
                    
                    remaining -= take;
                    self.segment_offset += take;
                    
                    if self.segment_offset >= *seg_len {
                        self.intermediate_pos += 1;
                        self.segment_offset = 0;
                    }
                }
                IntermediateSegment::Added { data } => {
                    let available = data.len() - self.segment_offset;
                    let take = remaining.min(available);
                    
                    self.result.push(DiffFragment::ADDED {
                        body: data[self.segment_offset..self.segment_offset + take].to_vec(),
                    });
                    
                    remaining -= take;
                    self.segment_offset += take;
                    
                    if self.segment_offset >= data.len() {
                        self.intermediate_pos += 1;
                        self.segment_offset = 0;
                    }
                }
            }
        }
    }
    
    fn process_deleted(&mut self, mut remaining: usize) {
        while remaining > 0 && self.intermediate_pos < self.segments.len() {
            match &self.segments[self.intermediate_pos] {
                IntermediateSegment::FromOriginal { offset: _, len: seg_len } => {
                    let available = seg_len - self.segment_offset;
                    let take = remaining.min(available);
                    
                    self.result.push(DiffFragment::DELETED { len: take });
                    
                    remaining -= take;
                    self.segment_offset += take;
                    
                    if self.segment_offset >= *seg_len {
                        self.intermediate_pos += 1;
                        self.segment_offset = 0;
                    }
                }
                IntermediateSegment::Added { data } => {
                    let available = data.len() - self.segment_offset;
                    let take = remaining.min(available);
                    
                    // Deleting something that was added in patch1
                    // means it never appears - no fragment needed
                    
                    remaining -= take;
                    self.segment_offset += take;
                    
                    if self.segment_offset >= data.len() {
                        self.intermediate_pos += 1;
                        self.segment_offset = 0;
                    }
                }
            }
        }
    }
    
    /// Consume the composer and return the final composed patch
    pub fn finish(self) -> Vec<DiffFragment> {
        merge_fragments(self.result)
    }
}

/// Compose two patches into a single patch (convenience function)
/// Given patch1 (A→B) and patch2 (B→C), returns a single patch (A→C)
/// For large patches, consider using PatchComposer directly for more control
pub fn compose_patches(
    patch1: &[DiffFragment],
    patch2: &[DiffFragment],
) -> Vec<DiffFragment> {
    let mut composer = PatchComposer::new();
    composer.apply_first_patch(patch1);
    composer.apply_second_patch(patch2);
    composer.finish()
}
