use std::io::{Read, Seek, Cursor};

use serde::{Serialize, Deserialize};

use crate::{
    diff::{self, DiffFragment},
    hash::Hash,
    objects::{Fragment, Object},
    repo::{Repo, BLOCK_SIZE},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileStruct {
    pub content_hash: Hash,
    pub fragments: Vec<Hash>,
}

pub type FileStructError = Box<dyn std::error::Error>;
pub type FileStructResult<T> = Result<T, FileStructError>;

impl FileStruct {
    /// Build a FileStruct from a seekable reader and an optional previous version
    ///
    /// # Arguments
    /// * `repo` - The repository to save fragment objects to
    /// * `reader` - A seekable reader for the current file content (will be read twice: once for hashing, once for diffing)
    /// * `prev_version` - Optional previous FileStruct to compute diff against
    ///
    /// # Returns
    /// * `Ok(FileStruct)` - The built FileStruct with content hash and fragment hashes
    ///   - If no previous version: creates a single ADDED fragment with entire file content
    ///   - If previous version exists: computes diff fragments between old and new versions
    /// * `Err` - If there was an error reading the file or saving fragments
    ///
    /// # Memory efficiency
    /// - Uses streaming hash to avoid loading file into memory for hashing
    /// - For diffs with previous version, reconstructs old file in memory (unavoidable with current diff algorithm)
    /// - diff_streams itself loads both files into memory (noted in its implementation)
    /// - Total memory for 10GB file with previous version: ~20-30GB (old + new + diff buffers)
    pub fn build<R: Read + Seek>(
        repo: &Repo,
        reader: &mut R,
        prev_version: Option<FileStruct>,
    ) -> FileStructResult<FileStruct> {
        // Compute content hash using streaming approach
        let content_hash = Hash::digest_file_stream(reader)?;

        // Rewind to beginning for diff computation
        reader.seek(std::io::SeekFrom::Start(0))?;

        // Determine diff fragments based on whether we have a previous version
        let diff_fragments = match prev_version {
            None => {
                // No previous version - read entire file as one ADDED fragment
                let mut body = Vec::new();
                reader.read_to_end(&mut body)?;
                vec![DiffFragment::ADDED { body }]
            }
            Some(prev_file_struct) => {
                // Previous version exists - reconstruct it and compute diff

                // Get all the fragment objects from the previous version
                let prev_fragments: Vec<Fragment> = prev_file_struct
                    .fragments
                    .into_iter()
                    .filter_map(|hash| {
                        let obj = repo.get_object(hash).ok()??;
                        if let Object::Fragment(frag) = obj {
                            Some(frag)
                        } else {
                            None
                        }
                    })
                    .collect();

                // Extract the DiffFragments from Fragment wrappers
                let prev_diff_fragments: Vec<DiffFragment> =
                    prev_fragments.into_iter().map(|f| f.0).collect();

                // Reconstruct the previous version in memory
                // NOTE: This loads the entire previous file into memory
                let mut prev_content = Vec::new();
                let empty_old = Cursor::new(Vec::<u8>::new());
                diff::apply_diff(empty_old, &prev_diff_fragments, &mut prev_content)?;

                // Compute the diff between previous and current version
                // NOTE: diff_streams loads both files into memory (see its implementation)
                let prev_cursor = Cursor::new(prev_content);
                diff::diff_streams(prev_cursor, reader, BLOCK_SIZE)?
            }
        };

        // Save each diff fragment as an object and collect the hashes
        let mut fragment_hashes = Vec::new();
        for frag in diff_fragments {
            let hash = repo.save_obj(Object::Fragment(Fragment(frag)))?;
            fragment_hashes.push(hash);
        }

        Ok(FileStruct {
            content_hash,
            fragments: fragment_hashes,
        })
    }
}
