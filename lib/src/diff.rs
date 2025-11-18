use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io::{Read, Write, Result, Seek, SeekFrom};

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

// Simple Rabin-Karp rolling hash
struct RollingHash {
    window: VecDeque<u8>,
    hash: u64,
    base: u64,
    modulus: u64,
    pow_base: u64,
}

impl RollingHash {
    fn new(base: u64, modulus: u64) -> Self {
        Self {
            window: VecDeque::new(),
            hash: 0,
            base,
            modulus,
            pow_base: 1,
        }
    }

    fn push(&mut self, byte: u8) {
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

    fn roll(&mut self, byte: u8) {
        if self.window.len() >= self.window.capacity() {
            self.pop();
        }
        self.push(byte);
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
    mut old: R1,
    mut new: R2,
    block_size: usize,
) -> Result<Vec<DiffFragment>> {
    // For very large files, we still need to read them into memory for the diff algorithm
    // The rolling hash algorithm requires random access to both files
    // Memory optimization: Consider implementing a chunked streaming diff for >1GB files
    let mut old_buffer = Vec::new();
    let mut new_buffer = Vec::new();

    old.read_to_end(&mut old_buffer)?;
    new.read_to_end(&mut new_buffer)?;

    // Step 1: Build signature table for old file
    // Hash every block of the old file and store in a HashMap
    let mut signatures: HashMap<u64, Vec<BlockSignature>> = HashMap::new();
    
    if old_buffer.len() >= block_size {
        for offset in 0..=(old_buffer.len() - block_size) {
            let block = &old_buffer[offset..offset + block_size];
            let mut hasher = RollingHash::new(257, 1_000_000_007);
            
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

    // Step 2: Scan through new file with rolling hash to find matches
    #[derive(Debug)]
    struct Match {
        old_offset: usize,
        new_offset: usize,
        len: usize,
    }
    
    let mut matches = Vec::new();
    let mut new_pos = 0;
    let mut used_ranges: Vec<(usize, usize)> = Vec::new(); // Track used ranges in old file

    while new_pos < new_buffer.len() {
        let mut found_match = false;

        // Only try to match if we have enough bytes for a block
        if new_pos + block_size <= new_buffer.len() {
            let block = &new_buffer[new_pos..new_pos + block_size];
            let mut hasher = RollingHash::new(257, 1_000_000_007);
            
            for &byte in block {
                hasher.push(byte);
            }
            
            let hash = hasher.digest();

            // Check if this hash exists in old file
            if let Some(candidates) = signatures.get(&hash) {
                // Find the best candidate (prefer earlier positions, avoid overlaps)
                let mut best_candidate: Option<&BlockSignature> = None;
                
                for candidate in candidates {
                    if candidate.data == block {
                        // Check if this range is already used
                        let is_used = used_ranges.iter().any(|(start, end)| {
                            candidate.offset < *end && candidate.offset + block_size > *start
                        });
                        
                        if !is_used {
                            best_candidate = Some(candidate);
                            break;
                        }
                    }
                }
                
                if let Some(candidate) = best_candidate {
                    // Try to extend the match beyond block_size
                    let mut match_len = block_size;
                    while candidate.offset + match_len < old_buffer.len()
                        && new_pos + match_len < new_buffer.len()
                        && old_buffer[candidate.offset + match_len]
                            == new_buffer[new_pos + match_len]
                    {
                        match_len += 1;
                    }

                    matches.push(Match {
                        old_offset: candidate.offset,
                        new_offset: new_pos,
                        len: match_len,
                    });
                    
                    used_ranges.push((candidate.offset, candidate.offset + match_len));
                    new_pos += match_len;
                    found_match = true;
                }
            }
        }

        if !found_match {
            new_pos += 1;
        }
    }

    // Step 3: Build the diff by walking through both files
    let mut final_diffs = Vec::new();
    let mut old_pos = 0;
    let mut new_pos = 0;

    for m in &matches {
        // Handle deletions (bytes in old file before this match that weren't matched)
        if old_pos < m.old_offset {
            final_diffs.push(DiffFragment::DELETED {
                len: m.old_offset - old_pos,
            });
        }

        // Handle additions (bytes in new file before this match that weren't matched)
        if new_pos < m.new_offset {
            final_diffs.push(DiffFragment::ADDED {
                body: new_buffer[new_pos..m.new_offset].to_vec(),
            });
        }

        // Handle the match
        final_diffs.push(DiffFragment::UNCHANGED { len: m.len });
        
        old_pos = m.old_offset + m.len;
        new_pos = m.new_offset + m.len;
    }

    // Handle remaining bytes at the end
    if old_pos < old_buffer.len() {
        final_diffs.push(DiffFragment::DELETED {
            len: old_buffer.len() - old_pos,
        });
    }
    
    if new_pos < new_buffer.len() {
        final_diffs.push(DiffFragment::ADDED {
            body: new_buffer[new_pos..].to_vec(),
        });
    }

    // Step 4: Merge consecutive fragments of the same type
    Ok(merge_fragments(final_diffs))
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
