use crate::{
    diff::DiffFragment,
    hash::Hash,
    objects::{
        CommitStruct, FileFragment, FileStruct,  Fragment, Object, ObjectReference,
    },
    replay::LazyFileReplay,
    space::ReadSeek,
    vlog,
};
use std::{error::Error, fs::File, io::Seek};
use vfs::FileSystem;

use std::io;

use crate::space::Space;

pub type FileOpsError = Box<dyn Error>;
pub type FileOpsResult<T> = Result<T, FileOpsError>;

pub struct FileStagedSummary {
    pub path: String,
    pub added_bytes: usize,
    pub deleted_bytes: usize,
    pub unchanged_bytes: usize,
}

pub struct FileOps<'a, F: FileSystem> {
    space: &'a mut Space<F>,
}

impl<'a, F: vfs::FileSystem> FileOps<'a, F> {
    pub fn from_space(space: &'a mut Space<F>) -> Self {
        Self { space }
    }

    pub fn stage_file<E, P>(
        &mut self,
        file_path: String,
        mut event: Option<E>,
        progress: Option<P>,
    ) -> FileOpsResult<Hash>
    where
        E: FnMut(DiffFragment),
        P: FnMut(crate::dedup::DedupProgress),
    {
        let fp = self.space.get_path_in_cwd_str(&file_path);

        let mut new = File::open(fp.clone())?;

        vlog!("space::stage_file: computing hash for entire file");
        let content_hash = Hash::digest_file_stream(&mut new)?;
        vlog!(
            "space::stage_file: entire file hash {}",
            content_hash.to_string()
        );

        let head_commit_hash = self
            .space
            .resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;

        let old: Box<dyn ReadSeek> = if head_commit_hash.is_zero() {
            vlog!("space::stage_file: no parent hash, using empty cursor");
            Box::new(io::Cursor::new(Vec::new()))
        } else {
            vlog!(
                "space::stage_file: opening parent version {}",
                head_commit_hash.to_string()
            );
            self.open_file(fp.clone(), head_commit_hash)?
        };

        new.seek(io::SeekFrom::Start(0))?;

        vlog!("space::stage_file: building diff fragments");
        let fragments = crate::dedup::build_diff_fragments(
            old,
            Box::new(new),
            self.space.chunk_size,
            self.space.max_size as u64,
            progress,
        )?;

        let mut file_fragments: Vec<Hash> = Vec::new();

        for fragment_res in fragments {
            if let Some(ref mut f) = event {
                f(fragment_res.clone());
            }

            match fragment_res {
                DiffFragment::ADDED { body } => {
                    for chunk in body.chunks(self.space.max_size) {
                        let frag_hash = self
                            .space
                            .save_obj(Object::Fragment(Fragment(chunk.to_vec())))?;
                        vlog!(
                            "space::stage_file: ADDED fragment chunk_size={} -> fragment_hash={}",
                            chunk.len(),
                            frag_hash.to_string()
                        );

                        let f = FileFragment::ADDED {
                            body: frag_hash,
                            len: chunk.len(),
                        };

                        let h = self.space.save_obj(Object::FileDiffFragment(f))?;
                        file_fragments.push(h);
                    }
                }
                DiffFragment::UNCHANGED { len } => {
                    vlog!("space::stage_file: UNCHANGED len={}", len);
                    let f = FileFragment::UNCHANGED { len };
                    let h = self.space.save_obj(Object::FileDiffFragment(f))?;
                    file_fragments.push(h);
                }
                DiffFragment::DELETED { len } => {
                    vlog!("space::stage_file: DELETED len={}", len);
                    let f = FileFragment::DELETED { len };
                    let h = self.space.save_obj(Object::FileDiffFragment(f))?;
                    file_fragments.push(h);
                }
            }
        }

        let version_hash = self.space.save_obj(Object::File(FileStruct {
            content_hash: content_hash,
            fragments: file_fragments,
        }))?;

        self.space.index.insert(fp.clone(), version_hash);
        vlog!(
            "space::stage_file: indexed '{}' -> {}",
            fp,
            version_hash.to_string()
        );

        Ok(version_hash)
    }

    pub fn unstage_file(&mut self, file_path: String) -> FileOpsResult<()> {
        let fp = self.space.get_path_in_cwd_str(&file_path);
        self.space.index.remove(fp.as_str());
        Ok(())
    }

    pub fn commit(&mut self, message: String) -> FileOpsResult<Hash> {
        vlog!("space::commit: message='{}'", message);

        let head_commit = self.space.get_head_commit_hash()?;

        let commit = CommitStruct {
            parent: head_commit,
            message: message,
            comitter: self.space.me.clone(),
            author: self.space.me.clone(),
            files: self.space.index.clone(),
        };

        let files_count = commit.files.len();
        vlog!("space::commit: creating commit with {} files", files_count);

        let commit_hash = self.space.save_obj(Object::Commit(commit))?;

        let head_ref_name = "HEAD".to_string();
        self.space.set_ref(
            head_ref_name.as_str(),
            ObjectReference::Hash(commit_hash.clone()),
        )?;
        vlog!("space::commit: new HEAD = {}", commit_hash.to_string());

        if files_count > 0 {
            self.space.index.clear();
            vlog!("space::commit: cleared {} entries from index", files_count);
            self.space.save_index()?;
        }

        Ok(commit_hash)
    }

    pub fn staged_summary(&self) -> FileOpsResult<Vec<FileStagedSummary>> {
        let mut result = Vec::new();
        for (path, &version_hash) in &self.space.index {
            let mut added = 0usize;
            let mut deleted = 0usize;
            let mut unchanged = 0usize;
            if let Some(Object::File(file)) = self.space.get_object(version_hash)? {
                for frag_hash in &file.fragments {
                    if let Some(Object::FileDiffFragment(frag)) = self.space.get_object(*frag_hash)? {
                        match frag {
                            FileFragment::ADDED { len, .. } => added += len,
                            FileFragment::DELETED { len } => deleted += len,
                            FileFragment::UNCHANGED { len } => unchanged += len,
                        }
                    }
                }
            }
            result.push(FileStagedSummary {
                path: path.clone(),
                added_bytes: added,
                deleted_bytes: deleted,
                unchanged_bytes: unchanged,
            });
        }
        Ok(result)
    }

    pub fn open_file(&mut self, file_path: String, hash: Hash) -> FileOpsResult<Box<dyn ReadSeek>> {
        vlog!(
            "space::open_file: file_path='{}' hash={}",
            file_path,
            hash.to_string()
        );
        let fp = self.space.get_path_in_cwd_str(&file_path);

        let commit = match self.space.get_object(hash)? {
            Some(Object::Commit(c)) => c,
            None => return Ok(Box::new(io::Cursor::new(Vec::<u8>::new()))),
            _ => return Err(Box::new(crate::error::DuhError::invalid_object("commit", "unknown object type"))),
        };

        let file_version_hash = match commit.files.get(fp.as_str()) {
            Some(x) => x,
            None => {
                vlog!(
                    "space::open_file: file '{}' not in commit {}",
                    fp,
                    hash.to_string()
                );
                return Ok(Box::new(io::Cursor::new(Vec::<u8>::new())));
            }
        };

        let file_version = match self.space.get_object(*file_version_hash)? {
            Some(Object::File(c)) => c,
            _ => return Err(Box::new(crate::error::DuhError::invalid_object("file", "unknown object type"))),
        };

        let mut fragments = Vec::with_capacity(file_version.fragments.len());
        for frag_hash in file_version.fragments {
            match self.space.get_object(frag_hash)? {
                Some(Object::FileDiffFragment(frag)) => fragments.push(frag),
                _ => return Err(Box::new(crate::error::DuhError::invalid_object("file diff fragment", "unknown object type"))),
            }
        }

        let parent_reader: Box<dyn ReadSeek> = if commit.parent.is_zero() {
            Box::new(io::Cursor::new(Vec::<u8>::new()))
        } else {
            self.open_file(fp.clone(), commit.parent)?
        };

        vlog!(
            "space::open_file: creating lazy replay with {} fragments, parent_hash={}",
            fragments.len(),
            commit.parent.to_string()
        );

        let replay = LazyFileReplay::new(&self.space, parent_reader, fragments)?;

        Ok(Box::new(replay))
    }
}
