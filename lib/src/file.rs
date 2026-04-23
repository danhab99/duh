use crate::{
    diff::DiffFragment,
    hash::Hash,
    objects::{CommitStruct, FileFragment, FileVersion, Fragment, Object, ObjectReference},
    replay::LazyFileReplay,
    repo::ReadSeek,
    vlog,
};
use std::{error::Error, fs::File, io::Seek};
use vfs::FileSystem;

use std::io;

use crate::repo::Repo;

pub type FileOpsError = Box<dyn Error>;
pub type FileOpsResult<T> = Result<T, FileOpsError>;

pub struct FileStagedSummary {
    pub path: String,
    pub added_bytes: usize,
    pub deleted_bytes: usize,
    pub unchanged_bytes: usize,
}

pub struct FileOps<F: FileSystem> {
    repo: Repo<F>,
}

impl<F: vfs::FileSystem> FileOps<F> {
    pub fn from_repo(repo: Repo<F>) -> Self {
        Self { repo }
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
        let fp = self.repo.get_path_in_cwd_str(&file_path);

        let mut new = File::open(fp.clone())?;

        vlog!("repo::stage_file: computing hash for entire file");
        let content_hash = Hash::digest_file_stream(&mut new)?;
        vlog!(
            "repo::stage_file: entire file hash {}",
            content_hash.to_string()
        );

        let head_commit_hash = self
            .repo
            .resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;

        let old: Box<dyn ReadSeek> = if head_commit_hash.is_zero() {
            vlog!("repo::stage_file: no parent hash, using empty cursor");
            Box::new(io::Cursor::new(Vec::new()))
        } else {
            vlog!(
                "repo::stage_file: opening parent version {}",
                head_commit_hash.to_string()
            );
            self.open_file(fp.clone(), head_commit_hash)?
        };

        new.seek(io::SeekFrom::Start(0))?;

        vlog!("repo::stage_file: building diff fragments");
        let fragments = crate::dedup::build_diff_fragments(
            old,
            Box::new(new),
            self.repo.chunk_size,
            self.repo.max_size as u64,
            progress,
        )?;

        let mut file_fragments: Vec<FileFragment> = Vec::new();

        for fragment_res in fragments {
            if let Some(ref mut f) = event {
                f(fragment_res.clone());
            }

            match fragment_res {
                DiffFragment::ADDED { body } => {
                    for chunk in body.chunks(self.repo.max_size) {
                        let frag_hash = self
                            .repo
                            .save_obj(Object::Fragment(Fragment(chunk.to_vec())))?;
                        vlog!(
                            "repo::stage_file: ADDED fragment chunk_size={} -> fragment_hash={}",
                            chunk.len(),
                            frag_hash.to_string()
                        );
                        file_fragments.push(FileFragment::ADDED {
                            body: frag_hash,
                            len: chunk.len(),
                        });
                    }
                }
                DiffFragment::UNCHANGED { len } => {
                    vlog!("repo::stage_file: UNCHANGED len={}", len);
                    file_fragments.push(FileFragment::UNCHANGED { len });
                }
                DiffFragment::DELETED { len } => {
                    vlog!("repo::stage_file: DELETED len={}", len);
                    file_fragments.push(FileFragment::DELETED { len });
                }
            }
        }

        let version = FileVersion {
            content_hash: content_hash,
            fragments: file_fragments,
        };

        let version_hash = self.repo.save_obj(Object::FileVersion(version))?;

        self.repo.index.insert(fp.clone(), version_hash);
        vlog!(
            "repo::stage_file: indexed '{}' -> {}",
            fp,
            version_hash.to_string()
        );

        Ok(version_hash)
    }

    pub fn unstage_file(&mut self, file_path: String) -> FileOpsResult<()> {
        let fp = self.repo.get_path_in_cwd_str(&file_path);
        self.repo.index.remove(fp.as_str());
        Ok(())
    }

    pub fn commit(&mut self, message: String) -> FileOpsResult<Hash> {
        vlog!("repo::commit: message='{}'", message);

        let head_commit = self.repo.get_head_commit_hash()?;

        let commit = CommitStruct {
            parent: head_commit,
            message: message,
            comitter: self.repo.me.clone(),
            author: self.repo.me.clone(),
            files: self.repo.index.clone(),
        };

        let files_count = commit.files.len();
        vlog!("repo::commit: creating commit with {} files", files_count);

        let commit_hash = self.repo.save_obj(Object::Commit(commit))?;

        let head_ref_name = "HEAD".to_string();
        self.repo.set_ref(
            head_ref_name.as_str(),
            ObjectReference::Hash(commit_hash.clone()),
        )?;
        vlog!("repo::commit: new HEAD = {}", commit_hash.to_string());

        if files_count > 0 {
            self.repo.index.clear();
            vlog!("repo::commit: cleared {} entries from index", files_count);
            self.repo.save_index()?;
        }

        Ok(commit_hash)
    }

    pub fn staged_summary(&self) -> FileOpsResult<Vec<FileStagedSummary>> {
        let mut result = Vec::new();
        for (path, &version_hash) in &self.repo.index {
            let mut added = 0usize;
            let mut deleted = 0usize;
            let mut unchanged = 0usize;
            if let Some(Object::FileVersion(fv)) = self.repo.get_object(version_hash)? {
                for frag in &fv.fragments {
                    match frag {
                        FileFragment::ADDED { len, .. } => added += len,
                        FileFragment::DELETED { len } => deleted += len,
                        FileFragment::UNCHANGED { len } => unchanged += len,
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

    pub fn open_file(
        &mut self,
        file_path: String,
        hash: Hash,
    ) -> FileOpsResult<Box<dyn ReadSeek>> {
        vlog!(
            "repo::open_file: file_path='{}' hash={}",
            file_path,
            hash.to_string()
        );
        let fp = self.repo.get_path_in_cwd_str(&file_path);

        let commit = match self.repo.get_object(hash)? {
            Some(Object::Commit(c)) => c,
            None => return Ok(Box::new(io::Cursor::new(Vec::<u8>::new()))),
            _ => panic!("expected commit object"),
        };

        let file_version_hash = match commit.files.get(fp.as_str()) {
            Some(x) => x,
            None => {
                vlog!(
                    "repo::open_file: file '{}' not in commit {}",
                    fp,
                    hash.to_string()
                );
                return Ok(Box::new(io::Cursor::new(Vec::<u8>::new())));
            }
        };

        let file_version = match self.repo.get_object(*file_version_hash)? {
            Some(Object::FileVersion(c)) => c,
            _ => panic!("expected file version"),
        };

        let parent_reader: Box<dyn ReadSeek> = if commit.parent.is_zero() {
            Box::new(io::Cursor::new(Vec::<u8>::new()))
        } else {
            self.open_file(fp.clone(), commit.parent)?
        };

        vlog!(
            "repo::open_file: creating lazy replay with {} fragments, parent_hash={}",
            file_version.fragments.len(),
            commit.parent.to_string()
        );

        let replay = LazyFileReplay::new(&self.repo, parent_reader, file_version.fragments)?;

        Ok(Box::new(replay))
    }
}
