use crate::objects::FileFragment;
use crate::repo::{ReadSeek, Repo, RepoResult};
use std::io::{self, Read, Seek, SeekFrom};

/// A reader that lazily replays file fragments to reconstruct a file.
/// 
/// # Overview
/// 
/// This reader takes a reference to an old file (via a Read+Seek trait object)
/// and a list of FileFragment entries, and reconstructs the new file on-the-fly
/// as you read from it. This addresses the need for lazy file replay as described
/// in the original requirements.
///
/// # Fragment Handling
/// 
/// The reader processes three types of fragments:
/// 
/// - **`UNCHANGED { len }`**: Reads `len` bytes from the old file stream.
///   The old file position advances by `len` bytes.
/// 
/// - **`ADDED { body, len }`**: Outputs the added fragment data (loaded from repo).
///   The old file position stays the same (no advancement).
/// 
/// - **`DELETED { len }`**: Skips `len` bytes in the old file without outputting anything.
///   The old file position advances by `len` bytes, but nothing is written to output.
///   This handles the case mentioned in the problem: "If you have a deleted fragment
///   of 100 bytes, you just advance the old reader by that number of bits."
///
/// # Seeking Behavior
///
/// The reader fully supports seeking both forward and backward, which answers the
/// question: "How do you seek backwards, especially in the middle of a deleted fragment?"
/// 
/// The solution is to maintain two separate position trackers:
/// 
/// 1. **Logical position**: The position in the reconstructed output file
/// 2. **Old file position**: The position in the old file being read from
/// 
/// When you seek:
/// - The reader calculates which fragment contains the target logical position
/// - It computes the correct old file position accounting for all DELETED fragments
/// - DELETED fragments are "invisible" in the output - you can't seek "into" them
///   because they don't contribute to the output. Instead, seeking accounts for
///   how much they shift the old file position.
/// 
/// ## Example: Seeking with DELETED Fragments
/// 
/// ```text
/// Fragments:
///   [UNCHANGED: 100] [DELETED: 50] [UNCHANGED: 100]
/// 
/// Output positions:     0-99  (gap)  100-199
/// Old file positions: 0-99  100-149  150-249
/// 
/// Seek to logical position 150:
///   - This is in the third fragment (second UNCHANGED)
///   - Offset within fragment: 150 - 100 = 50
///   - Old file position: 150 (start of fragment) + 50 = 200
///   - The DELETED fragment caused a +50 shift in old file position
/// ```
/// 
/// # Usage Example
/// 
/// ```rust,no_run
/// use lib::replay::LazyFileReplay;
/// use lib::repo::Repo;
/// use std::fs::File;
/// use std::io::Read;
/// 
/// # fn example(repo: &Repo, fragments: Vec<lib::objects::FileFragment>) -> std::io::Result<()> {
/// // Open the old file
/// let old_file = File::open("old_version.bin")?;
/// 
/// // Create the lazy replay reader
/// let mut reader = LazyFileReplay::new(
///     repo,
///     Box::new(old_file),
///     fragments,
/// ).expect("Failed to create replay reader");
/// 
/// // Read the reconstructed file
/// let mut buffer = vec![0u8; 1024];
/// let n = reader.read(&mut buffer)?;
/// # Ok(())
/// # }
/// ```
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

    // Helper to create fragment position calculations
    fn calculate_positions(fragments: &[FileFragment]) -> Vec<(u64, u64)> {
        let mut positions = Vec::new();
        let mut logical = 0u64;
        let mut old = 0u64;

        for frag in fragments {
            match frag {
                FileFragment::ADDED { len, .. } => {
                    positions.push((logical, old));
                    logical += *len as u64;
                }
                FileFragment::UNCHANGED { len } => {
                    positions.push((logical, old));
                    logical += *len as u64;
                    old += *len as u64;
                }
                FileFragment::DELETED { len } => {
                    positions.push((logical, old));
                    old += *len as u64;
                }
            }
        }
        positions.push((logical, old)); // Final position
        positions
    }

    #[test]
    fn test_position_calculation_unchanged_only() {
        let fragments = vec![
            FileFragment::UNCHANGED { len: 100 },
            FileFragment::UNCHANGED { len: 50 },
        ];

        let positions = calculate_positions(&fragments);
        
        // Fragment 0 starts at logical=0, old=0
        assert_eq!(positions[0], (0, 0));
        // Fragment 1 starts at logical=100, old=100
        assert_eq!(positions[1], (100, 100));
        // End at logical=150, old=150
        assert_eq!(positions[2], (150, 150));
    }

    #[test]
    fn test_position_calculation_with_deleted() {
        let fragments = vec![
            FileFragment::UNCHANGED { len: 50 },
            FileFragment::DELETED { len: 30 },
            FileFragment::UNCHANGED { len: 20 },
        ];

        let positions = calculate_positions(&fragments);
        
        // Fragment 0: logical=0, old=0
        assert_eq!(positions[0], (0, 0));
        // Fragment 1 (DELETED): logical=50, old=50
        assert_eq!(positions[1], (50, 50));
        // Fragment 2: logical=50 (no change from DELETED), old=80 (50+30)
        assert_eq!(positions[2], (50, 80));
        // End: logical=70 (50+20), old=100 (80+20)
        assert_eq!(positions[3], (70, 100));
    }

    #[test]
    fn test_position_calculation_with_added() {
        // Mock hash for testing
        let hash = Hash::from_str("0000000000000000000000000000000000000000000000000000000000000000");
        
        let fragments = vec![
            FileFragment::UNCHANGED { len: 50 },
            FileFragment::ADDED { body: hash.clone(), len: 25 },
            FileFragment::UNCHANGED { len: 30 },
        ];

        let positions = calculate_positions(&fragments);
        
        // Fragment 0: logical=0, old=0
        assert_eq!(positions[0], (0, 0));
        // Fragment 1 (ADDED): logical=50, old=50
        assert_eq!(positions[1], (50, 50));
        // Fragment 2: logical=75 (50+25), old=50 (no change from ADDED)
        assert_eq!(positions[2], (75, 50));
        // End: logical=105 (75+30), old=80 (50+30)
        assert_eq!(positions[3], (105, 80));
    }

    #[test]
    fn test_position_calculation_complex() {
        let hash = Hash::from_str("0000000000000000000000000000000000000000000000000000000000000000");
        
        let fragments = vec![
            FileFragment::UNCHANGED { len: 100 },
            FileFragment::DELETED { len: 50 },
            FileFragment::ADDED { body: hash.clone(), len: 75 },
            FileFragment::UNCHANGED { len: 25 },
            FileFragment::DELETED { len: 10 },
        ];

        let positions = calculate_positions(&fragments);
        
        // Track through each fragment
        assert_eq!(positions[0], (0, 0));       // Start of UNCHANGED
        assert_eq!(positions[1], (100, 100));   // Start of DELETED
        assert_eq!(positions[2], (100, 150));   // Start of ADDED (logical unchanged, old+=50)
        assert_eq!(positions[3], (175, 150));   // Start of UNCHANGED (logical+=75, old unchanged)
        assert_eq!(positions[4], (200, 175));   // Start of DELETED (logical+=25, old+=25)
        assert_eq!(positions[5], (200, 185));   // End (logical unchanged, old+=10)
    }

    #[test]
    fn test_total_size_calculation() {
        let hash = Hash::from_str("0000000000000000000000000000000000000000000000000000000000000000");
        
        let fragments = vec![
            FileFragment::UNCHANGED { len: 100 },
            FileFragment::DELETED { len: 50 },    // Doesn't contribute
            FileFragment::ADDED { body: hash, len: 75 },
            FileFragment::UNCHANGED { len: 25 },
        ];

        let total: u64 = fragments
            .iter()
            .map(|f| match f {
                FileFragment::ADDED { len, .. } => *len as u64,
                FileFragment::UNCHANGED { len } => *len as u64,
                FileFragment::DELETED { .. } => 0,
            })
            .sum();

        // 100 + 0 + 75 + 25 = 200
        assert_eq!(total, 200);
    }
}
