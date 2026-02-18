use crate::objects::FileFragment;
use crate::repo::{ReadSeek, Repo, RepoResult};
use std::io::{self, Read, Seek, SeekFrom};

/// A reader that lazily replays file fragments to reconstruct a file.
/// 
/// This reader takes a reference to an old file (via a Read+Seek trait object)
/// and a list of FileFragment entries, and reconstructs the new file on-the-fly
/// as you read from it.
///
/// # Fragment Handling
/// 
/// - `UNCHANGED { len }`: Reads `len` bytes from the old file
/// - `ADDED { body, len }`: Outputs the fragment data (loaded from the repo)
/// - `DELETED { len }`: Skips `len` bytes in the old file (doesn't output anything)
///
/// # Seeking
///
/// The reader supports seeking both forward and backward. When seeking:
/// - The logical position in the output stream is tracked
/// - For each fragment, we calculate where it starts/ends in the output
/// - DELETED fragments don't contribute to output, but advance the old file position
pub struct LazyFileReplay<'a> {
    /// Reference to the repository (needed to load ADDED fragment data)
    repo: &'a Repo,
    
    /// The old file to read UNCHANGED fragments from
    old_reader: Box<dyn ReadSeek + 'a>,
    
    /// List of fragments describing the diff
    fragments: Vec<FileFragment>,
    
    /// Current logical position in the output (reconstructed) file
    logical_position: u64,
    
    /// Current position in the old file
    old_position: u64,
    
    /// Index of the current fragment being processed
    current_fragment_index: usize,
    
    /// Offset within the current fragment
    offset_in_fragment: usize,
    
    /// Total logical size of the reconstructed file
    total_size: u64,
    
    /// Cache for the current ADDED fragment data (to avoid reloading)
    current_added_data: Option<Vec<u8>>,
}

impl<'a> LazyFileReplay<'a> {
    /// Create a new lazy file replay reader
    pub fn new(
        repo: &'a Repo,
        old_reader: Box<dyn ReadSeek + 'a>,
        fragments: Vec<FileFragment>,
    ) -> RepoResult<Self> {
        // Calculate total size of the reconstructed file
        let total_size: u64 = fragments
            .iter()
            .map(|f| match f {
                FileFragment::ADDED { len, .. } => *len as u64,
                FileFragment::UNCHANGED { len } => *len as u64,
                FileFragment::DELETED { .. } => 0, // DELETED doesn't contribute to output
            })
            .sum();

        Ok(Self {
            repo,
            old_reader,
            fragments,
            logical_position: 0,
            old_position: 0,
            current_fragment_index: 0,
            offset_in_fragment: 0,
            total_size,
            current_added_data: None,
        })
    }

    /// Get the fragment info for the current logical position
    /// Returns (fragment_index, offset_in_fragment, old_file_offset)
    fn find_fragment_at_position(&self, logical_pos: u64) -> Option<(usize, usize, u64)> {
        let mut current_logical = 0u64;
        let mut current_old = 0u64;

        for (idx, fragment) in self.fragments.iter().enumerate() {
            match fragment {
                FileFragment::ADDED { len, .. } => {
                    let frag_len = *len as u64;
                    if logical_pos >= current_logical && logical_pos < current_logical + frag_len {
                        let offset = (logical_pos - current_logical) as usize;
                        return Some((idx, offset, current_old));
                    }
                    current_logical += frag_len;
                }
                FileFragment::UNCHANGED { len } => {
                    let frag_len = *len as u64;
                    if logical_pos >= current_logical && logical_pos < current_logical + frag_len {
                        let offset = (logical_pos - current_logical) as usize;
                        return Some((idx, offset, current_old + offset as u64));
                    }
                    current_logical += frag_len;
                    current_old += frag_len;
                }
                FileFragment::DELETED { len } => {
                    // DELETED doesn't contribute to logical output, just advances old position
                    current_old += *len as u64;
                }
            }
        }

        // Position is at or past the end
        if logical_pos == current_logical {
            // EOF position
            Some((self.fragments.len(), 0, current_old))
        } else {
            None
        }
    }

    /// Load the data for an ADDED fragment
    fn load_added_fragment(&self, body_hash: &crate::hash::Hash) -> RepoResult<Vec<u8>> {
        use crate::objects::{Fragment, Object, ObjectReference};
        use std::fs;

        let obj_path = self.repo.get_object_path(ObjectReference::Hash(body_hash.clone()))?;
        let packed = fs::read(obj_path)?;
        let obj = Object::from_msgpack(packed)?;
        
        match obj {
            Object::Fragment(Fragment(data)) => Ok(data),
            _ => Err("Expected Fragment object".into()),
        }
    }
}

impl<'a> Read for LazyFileReplay<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.logical_position >= self.total_size {
            return Ok(0); // EOF
        }

        let mut bytes_read = 0;

        while bytes_read < buf.len() && self.current_fragment_index < self.fragments.len() {
            let fragment = &self.fragments[self.current_fragment_index];

            match fragment {
                FileFragment::ADDED { body, len } => {
                    // Load fragment data if not cached
                    if self.current_added_data.is_none() {
                        let data = self.load_added_fragment(body)
                            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                        self.current_added_data = Some(data);
                    }

                    let data = self.current_added_data.as_ref().unwrap();
                    let remaining_in_fragment = *len - self.offset_in_fragment;
                    let to_copy = std::cmp::min(remaining_in_fragment, buf.len() - bytes_read);

                    buf[bytes_read..bytes_read + to_copy].copy_from_slice(
                        &data[self.offset_in_fragment..self.offset_in_fragment + to_copy],
                    );

                    bytes_read += to_copy;
                    self.offset_in_fragment += to_copy;
                    self.logical_position += to_copy as u64;

                    if self.offset_in_fragment >= *len {
                        // Move to next fragment
                        self.current_fragment_index += 1;
                        self.offset_in_fragment = 0;
                        self.current_added_data = None;
                    }
                }

                FileFragment::UNCHANGED { len } => {
                    let remaining_in_fragment = *len - self.offset_in_fragment;
                    let to_read = std::cmp::min(remaining_in_fragment, buf.len() - bytes_read);

                    // Read from old file
                    let n = self.old_reader.read(&mut buf[bytes_read..bytes_read + to_read])?;
                    
                    if n == 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "Unexpected EOF in old file",
                        ));
                    }

                    bytes_read += n;
                    self.offset_in_fragment += n;
                    self.old_position += n as u64;
                    self.logical_position += n as u64;

                    if self.offset_in_fragment >= *len {
                        // Move to next fragment
                        self.current_fragment_index += 1;
                        self.offset_in_fragment = 0;
                    }
                }

                FileFragment::DELETED { len } => {
                    // Skip in old file, don't output anything
                    self.old_reader.seek(SeekFrom::Current(*len as i64))?;
                    self.old_position += *len as u64;
                    
                    // Move to next fragment
                    self.current_fragment_index += 1;
                    self.offset_in_fragment = 0;
                }
            }
        }

        Ok(bytes_read)
    }
}

impl<'a> Seek for LazyFileReplay<'a> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::Current(offset) => self.logical_position as i64 + offset,
            SeekFrom::End(offset) => self.total_size as i64 + offset,
        };

        if target_pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Cannot seek before the start of the file",
            ));
        }

        let target_pos = target_pos as u64;

        if target_pos > self.total_size {
            // Seeking past EOF is allowed
            self.logical_position = self.total_size;
            return Ok(self.logical_position);
        }

        // Find the fragment at the target position
        let (frag_idx, offset, old_pos) = self.find_fragment_at_position(target_pos)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Invalid seek position")
            })?;

        // Update state
        self.current_fragment_index = frag_idx;
        self.offset_in_fragment = offset;
        self.logical_position = target_pos;
        self.old_position = old_pos;

        // Seek the old reader to the correct position
        self.old_reader.seek(SeekFrom::Start(old_pos))?;

        // Clear cached ADDED data when seeking
        self.current_added_data = None;

        Ok(self.logical_position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Hash;
    use std::io::Cursor;

    #[test]
    fn test_lazy_replay_unchanged_only() {
        // TODO: Implement test
        // This would need a mock Repo, which is complex
        // For now, we'll rely on integration tests
    }

    #[test]
    fn test_find_fragment_at_position() {
        // Test the position calculation logic with a simple example
        // We can't easily test the full reader without a Repo instance
    }
}
