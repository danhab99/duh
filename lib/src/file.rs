use crate::{
    diff::DiffFragment,
    hash::Hash,
    objects::{CommitStruct, FileFragment, FileStruct, Fragment, Object, ObjectReference},
    replay::LazyFileReplay,
    space::ReadSeek,
    vlog,
};
use std::{error::Error, fs::File, io::Seek, path::{Component, Path, PathBuf}};

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

pub struct FileOps<'a> {
    space: &'a mut Space,
}

impl<'a> FileOps<'a> {
    pub fn from_space(space: &'a mut Space) -> Self {
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
        let repo_path = self.normalize_repo_path(&file_path)?;
        let fp = self.space.get_path_in_worktree_str(&repo_path)?;

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
            self.open_file(repo_path.clone(), head_commit_hash)?
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

        self.space.index.insert(repo_path.clone(), version_hash);
        vlog!(
            "space::stage_file: indexed '{}' -> {}",
            repo_path,
            version_hash.to_string()
        );

        Ok(version_hash)
    }

    pub fn unstage_file(&mut self, file_path: String) -> FileOpsResult<()> {
        let repo_path = self.normalize_repo_path(&file_path)?;
        self.space.index.remove(repo_path.as_str());
        Ok(())
    }

    pub fn commit(&mut self, message: String) -> FileOpsResult<Hash> {
        vlog!("space::commit: message='{}'", message);

        let head_commit = self.space.get_head_commit_hash()?;

        let tree_hash = self.space.build_tree(&self.space.index)?;

        let commit = CommitStruct {
            parent: head_commit,
            tree: tree_hash,
            message: message.clone(),
            comitter: self.space.me.clone(),
            author: self.space.me.clone(),
        };

        let files_count = self.space.index.len();
        vlog!("space::commit: creating commit with {} files", files_count);

        let commit_hash = self.space.save_obj(Object::Commit(commit))?;

        let head_ref = self.space.get_ref("HEAD".to_string())?;
        match head_ref {
            // Normal mode: HEAD points to a branch (e.g. main). Advance that branch.
            ObjectReference::Ref(branch) => {
                self.space.set_ref(
                    branch.as_str(),
                    ObjectReference::Hash(commit_hash.clone()),
                    Some(format!("commit: {}", message.clone()).as_str()),
                )?;
                vlog!(
                    "space::commit: advanced branch '{}' -> {}",
                    branch,
                    commit_hash.to_string()
                );
            }
            // Detached HEAD mode: keep detached behavior and move HEAD directly.
            _ => {
                self.space.set_ref(
                    "HEAD",
                    ObjectReference::Hash(commit_hash.clone()),
                    Some(format!("commit: {}", message.clone()).as_str()),
                )?;
                vlog!("space::commit: detached HEAD -> {}", commit_hash.to_string());
            }
        }

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
                    if let Some(Object::FileDiffFragment(frag)) =
                        self.space.get_object(*frag_hash)?
                    {
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
        let repo_path = self.normalize_repo_path(&file_path)?;
        let fp = self.space.get_path_in_worktree_str(&repo_path)?;

        let commit = match self.space.get_object(hash)? {
            Some(Object::Commit(c)) => c,
            None => {
                return Err(Box::new(crate::error::DuhError::object_not_found(
                    &hash.to_string(),
                    "commit",
                )));
            }
            _ => {
                return Err(Box::new(crate::error::DuhError::invalid_object(
                    "commit",
                    "unknown object type",
                )))
            }
        };

        let files = self.space.get_commit_files(hash)?;
        let file_version_hash = match files.get(repo_path.as_str()).or_else(|| files.get(fp.as_str())) {
            Some(x) => x,
            None => {
                vlog!(
                    "space::open_file: file '{}' not in commit {}",
                    repo_path,
                    hash.to_string()
                );
                return Err(Box::new(crate::error::DuhError::file_not_in_commit(
                    &repo_path,
                    &hash.to_string(),
                )));
            }
        };

        let file_version = match self.space.get_object(*file_version_hash)? {
            Some(Object::File(c)) => c,
            _ => {
                return Err(Box::new(crate::error::DuhError::invalid_object(
                    "file",
                    "unknown object type",
                )))
            }
        };

        let mut fragments = Vec::with_capacity(file_version.fragments.len());
        for frag_hash in file_version.fragments {
            match self.space.get_object(frag_hash)? {
                Some(Object::FileDiffFragment(frag)) => fragments.push(frag),
                _ => {
                    return Err(Box::new(crate::error::DuhError::invalid_object(
                        "file diff fragment",
                        "unknown object type",
                    )))
                }
            }
        }

        let parent_reader: Box<dyn ReadSeek> = if commit.parent.is_zero() {
            Box::new(io::Cursor::new(Vec::<u8>::new()))
        } else {
            self.open_file(repo_path.clone(), commit.parent)?
        };

        vlog!(
            "space::open_file: creating lazy replay with {} fragments, parent_hash={}",
            fragments.len(),
            commit.parent.to_string()
        );

        let replay = LazyFileReplay::new(&self.space, parent_reader, fragments)?;

        Ok(Box::new(replay))
    }

    fn normalize_repo_path(&self, input: &str) -> FileOpsResult<String> {
        let input_path = PathBuf::from(input);
        let rel = if input_path.is_absolute() {
            let worktree = self
                .space
                .worktree
                .as_ref()
                .ok_or_else(|| Box::new(crate::error::DuhError::BareRepoNoWorktree) as Box<dyn Error>)?;
            input_path
                .strip_prefix(worktree)
                .map_err(|_| format!("path '{}' is outside this worktree", input))?
                .to_path_buf()
        } else {
            input_path
        };

        let mut cleaned = PathBuf::new();
        for component in Path::new(&rel).components() {
            match component {
                Component::CurDir => {}
                Component::Normal(part) => cleaned.push(part),
                Component::ParentDir => {
                    return Err(format!("path '{}' cannot contain '..'", input).into())
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(format!("invalid repository path '{}'", input).into())
                }
            }
        }

        if cleaned.as_os_str().is_empty() {
            return Err("empty file path".into());
        }

        Ok(cleaned.to_string_lossy().to_string())
    }
}
