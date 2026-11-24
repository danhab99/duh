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
    /// Staged for deletion (the byte counts are then zero).
    pub removed: bool,
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
        event: Option<E>,
        progress: Option<P>,
    ) -> FileOpsResult<Option<Hash>>
    where
        E: FnMut(DiffFragment),
        P: FnMut(crate::dedup::DedupProgress),
    {
        let repo_path = self.normalize_repo_path(&file_path)?;
        let fp = self.space.get_path_in_worktree_str(&repo_path)?;
        let new = File::open(fp)?;
        self.stage_reader(repo_path, Box::new(new), event, progress)
    }

    /// Stage `new` as the content of `repo_path`, diffed against HEAD's
    /// version of that file (or against nothing if HEAD lacks it).
    /// Returns the staged version, or `None` when the content is identical
    /// to HEAD's (any staged change to the path is then dropped, as in git).
    pub fn stage_reader<E, P>(
        &mut self,
        repo_path: String,
        new: Box<dyn ReadSeek>,
        event: Option<E>,
        progress: Option<P>,
    ) -> FileOpsResult<Option<Hash>>
    where
        E: FnMut(DiffFragment),
        P: FnMut(crate::dedup::DedupProgress),
    {
        self.stage_reader_from(repo_path, None, new, event, progress)
    }

    /// Like `stage_reader`, but diff against HEAD's version of `base_path`
    /// (e.g. a file's name before a rename) instead of `repo_path`'s own.
    pub fn stage_reader_from<E, P>(
        &mut self,
        repo_path: String,
        base_path: Option<String>,
        mut new: Box<dyn ReadSeek>,
        mut event: Option<E>,
        progress: Option<P>,
    ) -> FileOpsResult<Option<Hash>>
    where
        E: FnMut(DiffFragment),
        P: FnMut(crate::dedup::DedupProgress),
    {
        let repo_path = self.normalize_repo_path(&repo_path)?;
        let base_path = match base_path {
            Some(b) => Some(self.normalize_repo_path(&b)?).filter(|b| *b != repo_path),
            None => None,
        };
        let diff_against = base_path.clone().unwrap_or_else(|| repo_path.clone());

        vlog!("space::stage_file: computing hash for entire file");
        let content_hash = Hash::digest_file_stream(&mut new)?;
        vlog!(
            "space::stage_file: entire file hash {}",
            content_hash.to_string()
        );

        let head_commit_hash = self
            .space
            .resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;

        if base_path.is_none() {
            if let Some(head_version) = self.version_in_commit(head_commit_hash, &repo_path)? {
                if let Some(Object::File(f)) = self.space.get_object(head_version)? {
                    if f.content_hash == content_hash {
                        vlog!("space::stage_file: '{}' unchanged from HEAD", repo_path);
                        self.space.index.remove(&repo_path);
                        return Ok(None);
                    }
                }
            }
        }

        let in_head = self.version_in_commit(head_commit_hash, &diff_against)?.is_some();
        if base_path.is_some() && !in_head {
            return Err(format!("rename source '{}' is not in HEAD", diff_against).into());
        }
        let old: Box<dyn ReadSeek> = if !in_head {
            vlog!("space::stage_file: '{}' not in HEAD, using empty cursor", diff_against);
            Box::new(io::Cursor::new(Vec::new()))
        } else {
            vlog!(
                "space::stage_file: opening parent version {}",
                head_commit_hash.to_string()
            );
            self.open_file(diff_against.clone(), head_commit_hash)?
        };

        new.seek(io::SeekFrom::Start(0))?;

        vlog!("space::stage_file: building diff fragments");
        let fragments = crate::dedup::build_diff_fragments(
            old,
            new,
            self.space.chunk_size,
            // Target average chunk size; max_size (10 MiB default) would make
            // most files a single chunk and defeat deduplication.
            self.space.chunk_size as u64,
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
            base_path,
        }))?;

        self.space.index.insert(repo_path.clone(), version_hash);
        vlog!(
            "space::stage_file: indexed '{}' -> {}",
            repo_path,
            version_hash.to_string()
        );

        Ok(Some(version_hash))
    }

    /// Stage the removal of `file_path` from the next commit. A file that
    /// was only staged (never committed) is simply dropped from the index.
    pub fn stage_deletion(&mut self, file_path: String) -> FileOpsResult<()> {
        let repo_path = self.normalize_repo_path(&file_path)?;
        let head = self.space.get_head_commit_hash()?;
        if self.version_in_commit(head, &repo_path)?.is_some() {
            self.space.index.insert(repo_path, Hash::new());
        } else if self.space.index.remove(&repo_path).is_none() {
            return Err(format!("'{}' is not tracked", repo_path).into());
        }
        Ok(())
    }

    pub fn unstage_file(&mut self, file_path: String) -> FileOpsResult<()> {
        let repo_path = self.normalize_repo_path(&file_path)?;
        self.space.index.remove(repo_path.as_str());
        Ok(())
    }

    pub fn commit(&mut self, message: String) -> FileOpsResult<Hash> {
        vlog!("space::commit: message='{}'", message);

        let head_commit = self.space.get_head_commit_hash()?;

        // Files untouched since HEAD carry forward; staged versions replace them.
        let mut files = if head_commit.is_zero() {
            Default::default()
        } else {
            self.space.get_commit_files(head_commit)?
        };
        files.extend(self.space.index.iter().map(|(k, v)| (k.clone(), *v)));
        // A zero hash in the index marks a staged deletion.
        files.retain(|_, v| !v.is_zero());
        let tree_hash = self.space.build_tree(&files)?;

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
                    Some(format!("save: {}", message.clone()).as_str()),
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
                    Some(format!("save: {}", message.clone()).as_str()),
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
                removed: version_hash.is_zero(),
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
        // Spaces from before paths were repo-relative keyed files by absolute
        // worktree path; only meaningful when there is a worktree.
        let legacy_key = self.space.get_path_in_worktree_str(&repo_path).ok();

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
        let file_version_hash = match files
            .get(repo_path.as_str())
            .or_else(|| legacy_key.as_deref().and_then(|k| files.get(k)))
        {
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
        let base_path = file_version.base_path.clone();

        let fragments = self.load_fragments(&file_version)?;

        // A version's fragments diff against the file as it was when staged,
        // i.e. in the parent of the commit that introduced the version. Commits
        // that merely carried the version forward must be skipped.
        let version = *file_version_hash;
        let mut base_commit = commit.parent;
        while self.version_in_commit(base_commit, &repo_path)? == Some(version) {
            base_commit = match self.space.get_object(base_commit)? {
                Some(Object::Commit(c)) => c.parent,
                _ => {
                    return Err(Box::new(crate::error::DuhError::invalid_object(
                        "commit",
                        "unknown object type",
                    )))
                }
            };
        }

        // The diff base is the same path unless the version was staged
        // against another one (a rename).
        let base_path = base_path.unwrap_or_else(|| repo_path.clone());
        let parent_reader: Box<dyn ReadSeek> =
            if self.version_in_commit(base_commit, &base_path)?.is_none() {
                // File was added by the introducing commit: diff against nothing.
                Box::new(io::Cursor::new(Vec::<u8>::new()))
            } else {
                self.open_file(base_path, base_commit)?
            };

        vlog!(
            "space::open_file: creating lazy replay with {} fragments, base_commit={}",
            fragments.len(),
            base_commit.to_string()
        );

        let replay = LazyFileReplay::new(&self.space, parent_reader, fragments)?;

        Ok(Box::new(replay))
    }

    /// The diff fragments of a file version, in order.
    pub fn load_fragments(&self, version: &FileStruct) -> FileOpsResult<Vec<FileFragment>> {
        let mut fragments = Vec::with_capacity(version.fragments.len());
        for frag_hash in &version.fragments {
            match self.space.get_object(*frag_hash)? {
                Some(Object::FileDiffFragment(frag)) => fragments.push(frag),
                _ => {
                    return Err(Box::new(crate::error::DuhError::invalid_object(
                        "file diff fragment",
                        "unknown object type",
                    )))
                }
            }
        }
        Ok(fragments)
    }

    /// Read the version of `file_path` staged in the index. Staged versions
    /// diff against HEAD (at the file's own path, or its rename source).
    pub fn open_staged(&mut self, file_path: String) -> FileOpsResult<Box<dyn ReadSeek>> {
        let repo_path = self.normalize_repo_path(&file_path)?;
        let version_hash = match self.space.index.get(&repo_path) {
            Some(h) if !h.is_zero() => *h,
            _ => return Err(format!("'{}' has no staged version", repo_path).into()),
        };
        let version = match self.space.get_object(version_hash)? {
            Some(Object::File(f)) => f,
            _ => {
                return Err(Box::new(crate::error::DuhError::invalid_object(
                    "file",
                    "unknown object type",
                )))
            }
        };
        let fragments = self.load_fragments(&version)?;

        let head = self.space.get_head_commit_hash()?;
        let base_path = version.base_path.clone().unwrap_or(repo_path);
        let base: Box<dyn ReadSeek> = if self.version_in_commit(head, &base_path)?.is_some() {
            self.open_file(base_path, head)?
        } else {
            Box::new(io::Cursor::new(Vec::<u8>::new()))
        };
        Ok(Box::new(LazyFileReplay::new(&self.space, base, fragments)?))
    }

    /// The file version recorded for `repo_path` in `commit`, if any.
    fn version_in_commit(&self, commit: Hash, repo_path: &str) -> FileOpsResult<Option<Hash>> {
        if commit.is_zero() {
            return Ok(None);
        }
        Ok(self.space.get_commit_files(commit)?.get(repo_path).copied())
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
                .map_err(|_| format!("'{}' is outside this space", input))?
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::TempDir;

    fn space_with_worktree() -> (TempDir, Space) {
        let dir = TempDir::new().unwrap();
        let meta = dir.path().join(crate::utils::SPACE_METADATA_DIR_NAME);
        std::fs::create_dir_all(&meta).unwrap();
        let store = crate::store::local(&meta).unwrap();
        let space = Space::init(store, Some(dir.path().to_path_buf())).unwrap();
        (dir, space)
    }

    fn write(dir: &TempDir, path: &str, data: &[u8]) {
        let p = dir.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }

    fn stage(space: &mut Space, path: &str) {
        FileOps::from_space(space)
            .stage_file(path.to_string(), None::<fn(DiffFragment)>, None::<fn(_)>)
            .unwrap();
    }

    fn commit(space: &mut Space) -> Hash {
        FileOps::from_space(space).commit("test".to_string()).unwrap()
    }

    fn read_at(space: &mut Space, path: &str, commit: Hash) -> Option<Vec<u8>> {
        let mut reader = FileOps::from_space(space)
            .open_file(path.to_string(), commit)
            .ok()?;
        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        Some(out)
    }

    /// Interleaved edits to two files: every file must read back exactly as
    /// it was at every commit, whether it was edited, added later, carried
    /// forward untouched, or reverted.
    #[test]
    fn multi_file_history_round_trips() {
        let (dir, mut space) = space_with_worktree();
        // Non-repeating data, so replaying a diff onto the wrong base can't
        // reproduce the right bytes by coincidence.
        let noise = |seed: u64, len: usize| -> Vec<u8> {
            let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            (0..len)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    (x >> 24) as u8
                })
                .collect()
        };
        let a1 = noise(1, 50_000);
        let mut a2 = a1.clone();
        a2.splice(20_000..20_000, noise(2, 5_000));
        let b1 = noise(3, 40_000);
        let mut b2 = b1.clone();
        b2.extend_from_slice(&noise(4, 3_000));

        // Each step: (files to write and stage, expected state after commit).
        let steps: Vec<(Vec<(&str, &Vec<u8>)>, Vec<(&str, &Vec<u8>)>)> = vec![
            (vec![("a.bin", &a1)], vec![("a.bin", &a1)]),
            (vec![("sub/b.bin", &b1)], vec![("a.bin", &a1), ("sub/b.bin", &b1)]),
            (vec![("a.bin", &a2)], vec![("a.bin", &a2), ("sub/b.bin", &b1)]),
            (vec![("sub/b.bin", &b2)], vec![("a.bin", &a2), ("sub/b.bin", &b2)]),
            (vec![], vec![("a.bin", &a2), ("sub/b.bin", &b2)]),
            (vec![("a.bin", &a1)], vec![("a.bin", &a1), ("sub/b.bin", &b2)]),
        ];

        let mut history = Vec::new();
        for (writes, expected) in &steps {
            for (path, data) in writes {
                write(&dir, path, data);
                stage(&mut space, path);
            }
            history.push((commit(&mut space), expected));
        }

        for (i, (hash, expected)) in history.iter().enumerate() {
            assert_eq!(space.get_commit_files(*hash).unwrap().len(), expected.len());
            for (path, data) in expected.iter() {
                let got = read_at(&mut space, path, *hash)
                    .unwrap_or_else(|| panic!("commit {}: {} missing", i + 1, path));
                assert!(got == **data, "commit {}: {} differs", i + 1, path);
            }
        }

        assert!(read_at(&mut space, "sub/b.bin", history[0].0).is_none());

        // Abbreviated hashes resolve to the unique matching commit.
        for (hash, _) in &history {
            let short = ObjectReference::from(hash.to_string()[..10].to_string());
            assert_eq!(space.resolve_ref_name(short).unwrap(), *hash);
        }
        assert!(space.resolve_commit_prefix("zzzz").is_err());
    }

    fn read_all(mut r: Box<dyn ReadSeek>) -> Vec<u8> {
        let mut out = Vec::new();
        r.read_to_end(&mut out).unwrap();
        out
    }

    /// Staged versions read back exactly; a rename staged against its source
    /// reads back before and after commit, while the source is deleted.
    #[test]
    fn staged_versions_and_renames_round_trip() {
        let (dir, mut space) = space_with_worktree();
        let a1: Vec<u8> = (0..60_000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        let mut a2 = a1.clone();
        a2.extend_from_slice(b"more");

        write(&dir, "a.bin", &a1);
        stage(&mut space, "a.bin");
        commit(&mut space);

        write(&dir, "a.bin", &a2);
        stage(&mut space, "a.bin");
        let staged = FileOps::from_space(&mut space).open_staged("a.bin".into()).unwrap();
        assert!(read_all(staged) == a2);
        FileOps::from_space(&mut space).unstage_file("a.bin".into()).unwrap();

        // Restaging content identical to HEAD stages nothing.
        write(&dir, "a.bin", &a1);
        let none = FileOps::from_space(&mut space)
            .stage_file("a.bin".into(), None::<fn(DiffFragment)>, None::<fn(_)>)
            .unwrap();
        assert!(none.is_none() && space.index.is_empty());

        // Rename a.bin -> moved/a.bin, diffing against a.bin's content.
        std::fs::create_dir_all(dir.path().join("moved")).unwrap();
        std::fs::rename(dir.path().join("a.bin"), dir.path().join("moved/a.bin")).unwrap();
        let mut ops = FileOps::from_space(&mut space);
        ops.stage_deletion("a.bin".into()).unwrap();
        ops.stage_reader_from(
            "moved/a.bin".into(),
            Some("a.bin".into()),
            Box::new(std::fs::File::open(dir.path().join("moved/a.bin")).unwrap()),
            None::<fn(DiffFragment)>,
            None::<fn(_)>,
        )
        .unwrap();
        let staged = FileOps::from_space(&mut space).open_staged("moved/a.bin".into()).unwrap();
        assert!(read_all(staged) == a1);

        let renamed = commit(&mut space);
        let files = space.get_commit_files(renamed).unwrap();
        assert!(!files.contains_key("a.bin") && files.contains_key("moved/a.bin"));
        assert!(read_at(&mut space, "moved/a.bin", renamed).unwrap() == a1);
    }
}
