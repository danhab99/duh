use crate::{
    diff::DiffFragment,
    hash::Hash,
    objects::{CommitStruct, FileFragment, FileVersion, Fragment, Object, ObjectReference, Person},
    utils::{self, find_file, getRepoConfigFileName, REPO_METADATA_DIR_NAME},
};
use std::{
    collections::HashMap,
    error::Error,
    fs::{self, File},
    io::{Read, Seek},
    path::PathBuf,
    time::SystemTime,
};

use serde::Serialize;
use std::env;
use std::io;

// Object-safe alias for `Read + Seek` so we can store boxed readers that support both.
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek + ?Sized> ReadSeek for T {}

use toml;

pub struct Repo {
    root_path: String,
    buffer_size: usize,
    me: Person,
    index: HashMap<String, Hash>,
}

pub type RepoError = Box<dyn Error>;
pub type RepoResult<T> = Result<T, RepoError>;

pub const BLOCK_SIZE: usize = 512;

/// Maximum size (bytes) for a single stored ADDED fragment. Larger ADDED
/// bodies are split into multiple Fragment objects at stage time.
pub const MAX_FRAGMENT_SIZE: usize = 10 * 1024 * 1024; // 10 MiB

impl Repo {
    pub fn at_root_path(root_path: Option<String>) -> RepoResult<Repo> {
        let rp = match root_path {
            Some(x) => x,
            None => {
                let cwd = env::current_dir()?;
                let c = cwd.to_str().ok_or("cannot identify dir")?;
                String::from(c)
            }
        };

        let config_path = find_file(rp.as_str(), &getRepoConfigFileName())?;

        let content = fs::read(config_path)?;
        let decoded = String::from_utf8(content)?;
        let config = decoded.parse::<toml::Table>()?;

        let user_config = config
            .get("user")
            .ok_or("missing user config")?
            .as_table()
            .ok_or("user config isn't a table")?;

        let buffer_size = config
            .get("chunk_size")
            .ok_or("missing chunk_size")?
            .as_integer()
            .ok_or("chunk_size is not a number")? as usize;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let metadata_path = find_file(rp.as_str(), REPO_METADATA_DIR_NAME)?;
        // metadata_path == "<repo-root>/.duh" — store the repo root (parent of .duh)
        let repo_root = PathBuf::from(&metadata_path)
            .parent()
            .and_then(|p| p.to_str())
            .ok_or("couldn't determine repo root")?
            .to_string();

        let mut r = Repo {
            root_path: repo_root,
            buffer_size,
            me: Person {
                name: String::from(
                    user_config
                        .get("name")
                        .ok_or("missing user.name")?
                        .as_str()
                        .unwrap_or("user.name is not a string"),
                ),
                email: String::from(
                    user_config
                        .get("email")
                        .ok_or("missing user.email")?
                        .as_str()
                        .unwrap_or("user.email is not a string"),
                ),
                timestamp: now,
            },
            index: HashMap::new(),
        };

        let index_file_path = r.get_path_in_repo("index");

        let mut index_file = fs::File::open(index_file_path).unwrap();
        let mut contents = String::new();
        index_file.read_to_string(&mut contents)?;
        for line in contents.lines() {
            let parts = line.split("=").collect::<Vec<_>>();
            assert!(parts.len() == 2);

            let filepath_part = parts[0];
            let hash_part = parts[1];

            r.index.insert(
                filepath_part.to_string(),
                Hash::from_string(hash_part.to_string()),
            );
        }

        Ok(r)
    }

    fn get_path_in_repo(&self, p: &str) -> PathBuf {
        // returns `${root_path}/.duh/<p>` and ensures the metadata dir exists
        let mut b = PathBuf::from(self.root_path.clone());
        b.push(utils::REPO_METADATA_DIR_NAME);
        fs::create_dir_all(&b).unwrap();
        b.push(p);
        return b;
    }

    fn get_path_in_repo_str(&self, p: &str) -> String {
        let b = self.get_path_in_repo(p);
        let s = b.into_os_string().into_string().unwrap();
        return s;
    }

    pub fn get_path_in_cwd(&self, p: &str) -> PathBuf {
        PathBuf::from(utils::get_cwd()).join(p)
        // PathBuf::from(self.root_path.clone())
        //     .join(utils::get_cwd())
        //     .join(p)
    }

    pub fn get_path_in_cwd_str(&self, p: &str) -> String {
        let b = self.get_path_in_cwd(p);
        let s = b.into_os_string().into_string().unwrap();
        return s;
    }

    pub fn initialize_at(root_path: String) -> RepoResult<Repo> {
        // Create the metadata directory tree under the provided root path so
        // `at_root_path(Some(root_path))` can locate it reliably (don't rely on CWD).
        let base = PathBuf::from(root_path.clone()).join(utils::REPO_METADATA_DIR_NAME);
        fs::create_dir_all(base.join("objects"))?;
        fs::create_dir_all(base.join("refs"))?;

        // Write a minimal, valid TOML config so `at_root_path` can parse required fields.
        let default_config = format!(
            "chunk_size = {}\n\n[user]\nname = \"test\"\nemail = \"test@example.com\"\n",
            BLOCK_SIZE
        );
        fs::write(base.join("config"), default_config)?;

        // Initialize HEAD and index (also create refs/HEAD so lookups succeed)
        fs::write(base.join("HEAD"), "")?;
        fs::write(base.join("index"), "")?;
        fs::create_dir_all(base.join("refs"))?;
        fs::write(base.join("refs").join("HEAD"), "")?;

        Ok(Repo::at_root_path(Some(root_path))?)
    }

    fn get_object_path(&self, r: ObjectReference) -> RepoResult<PathBuf> {
        let hash = self.resolve_ref_name(r)?.to_string();
        let top = &hash[0..2];
        let bottom = &hash[2..hash.len()];

        Ok(self.get_path_in_repo(format!("objects/{}/{}", top, bottom).as_str()))
    }

    pub fn save_obj(&self, o: Object) -> RepoResult<Hash> {
        use rmp_serde::Serializer;
        use sha2::{Digest, Sha256};
        use std::io::Write as IoWrite;
        use tempfile::NamedTempFile;

        // Ensure objects dir exists
        let objects_dir = self.get_path_in_repo("objects");
        fs::create_dir_all(&objects_dir)?;

        // Create a named temp file inside the objects dir so rename/persist is atomic
        let mut tmp = NamedTempFile::new_in(&objects_dir)?;

        // Hasher to compute hash of the *msgpack* bytes as they are written
        let mut hasher = Sha256::new();

        // A small writer that writes to the temp file and updates the hasher
        struct HashingWriter<'a, W: IoWrite> {
            inner: &'a mut W,
            hasher: &'a mut Sha256,
        }

        impl<'a, W: IoWrite> IoWrite for HashingWriter<'a, W> {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.inner.write_all(buf)?;
                self.hasher.update(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.inner.flush()
            }
        }

        // Serialize the object directly into the temp file while hashing
        {
            let mut hw = HashingWriter {
                inner: &mut tmp,
                hasher: &mut hasher,
            };
            let mut ser = Serializer::new(&mut hw);
            o.serialize(&mut ser).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::Other, format!("serialize: {}", e))
            })?;
            hw.flush()?;
        }

        // finalize hash and build Hash value (hex-encoded sha256)
        let digest = hasher.finalize();
        let hex = hex::encode(digest);
        let hash = Hash::from_string(hex);

        // Persist temp file to final object path
        let path = self.get_object_path(ObjectReference::Hash(hash.clone()))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        tmp.persist(&path)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        // NOTE: `.raw` sidecars removed — fragment bytes are stored only inside the
        // serialized `Fragment` object on disk and will be deserialized when first read.
        Ok(hash)
    }

    fn read_object(&self, r: Hash) -> RepoResult<Option<Vec<u8>>> {
        let path = self.get_object_path(ObjectReference::Hash(r))?;
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read(path)?;
        return Ok(Some(content.to_vec()));
    }

    pub fn get_object(&self, r: Hash) -> RepoResult<Option<Object>> {
        let content = self.read_object(r)?;
        match content {
            Some(content) => Ok(Some(Object::from_msgpack(content)?)),
            None => Ok(None),
        }
    }

    fn get_ref_path(&self, name: &str) -> PathBuf {
        return self.get_path_in_repo(format!("refs/{}", name).as_str());
    }

    pub fn set_ref(&self, name: &str, r: ObjectReference) -> RepoResult<()> {
        let path = self.get_ref_path(name);
        fs::write(path, r.to_string())?;
        return Ok(());
    }

    pub fn get_ref(&self, name: String) -> RepoResult<ObjectReference> {
        let ref_path = self.get_path_in_repo(format!("refs/{}", name).as_str());
        let val = String::from_utf8(fs::read(ref_path)?)?;

        // Treat an empty ref file as "no parent" (map to zero hash) to make the
        // first commit/HEAD behaviour safe.
        if val.trim().is_empty() {
            return Ok(ObjectReference::Hash(Hash::new()));
        }

        Ok(ObjectReference::from(val))
    }

    pub fn resolve_ref_name(&self, ref_name: ObjectReference) -> RepoResult<Hash> {
        match ref_name {
            ObjectReference::Hash(h) => Ok(h),
            ObjectReference::Ref(r) => {
                let n = self.get_ref(r)?;
                return Ok(self.resolve_ref_name(n)?);
            }
        }
    }

    pub fn stage_file(&mut self, file_path: String) -> RepoResult<Hash> {
        let fp = self.get_path_in_cwd_str(&file_path);

        println!("stage_file: opening '{}'", fp);
        let mut new = File::open(fp.clone())?;
        println!("stage_file: opened");

        let content_hash = Hash::digest_file_stream(&mut new)?;
        println!("stage_file: content hash computed");

        let head_commit_hash = self.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;
        println!(
            "stage_file: head_commit_hash = {}",
            head_commit_hash.to_string()
        );

        let old: Box<dyn ReadSeek> = if head_commit_hash.is_zero() {
            println!("stage_file: stub file",);
            Box::new(tempfile::tempfile()?)
        } else {
            println!(
                "stage_file: open prev file {}",
                head_commit_hash.to_string()
            );
            self.open_file(fp.clone(), head_commit_hash)?
        };

        println!("stage_file: opened old file reader");

        new.seek(io::SeekFrom::Start(0))?;

        let fragments = crate::diff::build_diff_fragments(old, Box::new(new), BLOCK_SIZE);

        // Collect `FileFragment` entries directly in the FileVersion so we avoid
        // creating a separate FileDiffFragment object for every ADDED fragment.
        let mut file_fragments: Vec<FileFragment> = Vec::new();

        for fragment_res in fragments {
            match fragment_res {
                Ok(DiffFragment::ADDED { body }) => {
                    // split large ADDED bodies into FRAGMENT-sized chunks
                    for chunk in body.chunks(MAX_FRAGMENT_SIZE) {
                        let frag_hash =
                            self.save_obj(Object::Fragment(Fragment(chunk.to_vec())))?;
                        file_fragments.push(FileFragment::ADDED {
                            body: frag_hash,
                            len: chunk.len(),
                        });
                    }
                }
                Ok(DiffFragment::UNCHANGED { len }) => {
                    file_fragments.push(FileFragment::UNCHANGED { len });
                }
                Ok(DiffFragment::DELETED { len }) => {
                    file_fragments.push(FileFragment::DELETED { len });
                }
                Err(x) => panic!("{}", x),
            }
        }

        let version = FileVersion {
            content_hash: content_hash,
            fragments: file_fragments,
        };

        let version_hash = self.save_obj(Object::FileVersion(version))?;

        self.index.insert(fp.to_string(), version_hash);

        Ok(version_hash)
    }

    pub fn commit(&mut self, message: String) -> RepoResult<Hash> {
        let head_commit = self.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;

        let commit = CommitStruct {
            parent: head_commit,
            message: message,
            comitter: self.me.clone(),
            author: self.me.clone(),
            files: self.index.clone(),
        };

        let commit_hash = self.save_obj(Object::Commit(commit))?;

        self.set_ref("HEAD", ObjectReference::Hash(commit_hash))?;

        Ok(commit_hash)
    }

    pub fn open_file(&mut self, file_path: String, hash: Hash) -> RepoResult<Box<dyn ReadSeek>> {
        // Resolve the commit pointed to by `hash`; if it doesn't exist, return an empty reader.
        let fp = self.get_path_in_cwd_str(&file_path);

        let commit = match self.get_object(hash)? {
            Some(Object::Commit(c)) => c,
            None => return Ok(Box::new(io::Cursor::new(Vec::<u8>::new()))),
            _ => panic!("expected commit object"),
        };

        // Open the parent file (or an empty cursor if there is no parent commit).
        let mut parent_reader: Box<dyn ReadSeek> = match self.get_object(commit.parent)? {
            Some(Object::Commit(_)) => self.open_file(fp.clone(), commit.parent)?,
            None => Box::new(io::Cursor::new(Vec::<u8>::new())),
            _ => panic!("expected commit object for parent"),
        };

        for (path, file) in &commit.files {
            println!("File path: {}", path);
            println!("File info: {:#?}", file);
        }

        let file_version_hash = match commit.files.get(fp.as_str()) {
            Some(x) => x,
            _ => panic!("file does not exist in this commit {}", fp.as_str()),
        };

        let file_version = match self.get_object(*file_version_hash)? {
            Some(Object::FileVersion(c)) => c,
            _ => panic!("expected file version"),
        };

        // Build an index of fragments describing how to materialize this file.
        let mut spans: Vec<FragmentSpan> = Vec::new();
        let mut output_cursor: u64 = 0;
        let mut parent_cursor: u64 = 0;

        for ff in file_version.fragments.iter() {
            match ff {
                FileFragment::ADDED { body, len } => {
                    // don't load the fragment body into RAM here — store object path + length
                    let obj_path = self.get_object_path(ObjectReference::Hash(body.clone()))?;
                    let len_u64 = *len as u64;
                    spans.push(FragmentSpan {
                        output_start: output_cursor,
                        output_end: output_cursor + len_u64,
                        kind: FragmentKind::Added {
                            path: obj_path,
                            len: len_u64,
                            hash: body.clone(),
                            cache: None,
                        },
                    });

                    output_cursor += len_u64;
                }
                FileFragment::UNCHANGED { len } => {
                    let len_u64 = *len as u64;
                    spans.push(FragmentSpan {
                        output_start: output_cursor,
                        output_end: output_cursor + len_u64,
                        kind: FragmentKind::Unchanged {
                            parent_offset: parent_cursor,
                        },
                    });

                    output_cursor += len_u64;
                    parent_cursor += len_u64;
                }
                FileFragment::DELETED { len } => {
                    parent_cursor += *len as u64;
                }
            }
        }

        let reader = RepoSeekableFile::new(spans, output_cursor, parent_reader);

        Ok(Box::new(reader))
    }
}

/// Represents a byte-producing slice of the reconstructed file.
struct FragmentSpan {
    output_start: u64,
    output_end: u64,
    kind: FragmentKind,
}

enum FragmentKind {
    Added {
        path: PathBuf,
        hash: Hash,
        len: u64,
        cache: Option<Vec<u8>>,
    },
    Unchanged {
        parent_offset: u64,
    },
}

struct RepoSeekableFile {
    spans: Vec<FragmentSpan>,
    pos: u64,
    total_len: u64,
    parent: Box<dyn ReadSeek>,
}

impl RepoSeekableFile {
    fn new(spans: Vec<FragmentSpan>, total_len: u64, parent: Box<dyn ReadSeek>) -> Self {
        Self {
            spans,
            pos: 0,
            total_len,
            parent,
        }
    }

    fn find_span_index(&self, offset: u64) -> Option<usize> {
        if self.spans.is_empty() {
            return None;
        }

        let mut lo = 0usize;
        let mut hi = self.spans.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            let span = &self.spans[mid];
            if offset < span.output_start {
                hi = mid;
            } else if offset >= span.output_end {
                lo = mid + 1;
            } else {
                return Some(mid);
            }
        }
        None
    }
}

impl std::io::Read for RepoSeekableFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        if self.pos >= self.total_len {
            return Ok(0);
        }

        let mut written = 0usize;
        while written < buf.len() && self.pos < self.total_len {
            let span_idx = match self.find_span_index(self.pos) {
                Some(i) => i,
                None => break,
            };

            let span = &mut self.spans[span_idx];
            let span_offset = self.pos - span.output_start;
            let span_remaining = (span.output_end - self.pos) as usize;
            let dest_remaining = buf.len() - written;
            let to_xfer = std::cmp::min(span_remaining, dest_remaining);

            match &mut span.kind {
                FragmentKind::Added {
                    path,
                    len: _,
                    hash: _,
                    cache,
                } => {
                    // Load fragment bytes from the serialized `Fragment` object on first access.
                    if cache.is_none() {
                        let packed = fs::read(path)?;
                        let obj = Object::from_msgpack(packed).map_err(|e| {
                            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                        })?;
                        let frag_bytes = match obj {
                            Object::Fragment(Fragment(b)) => b,
                            _ => panic!("expected Fragment body"),
                        };
                        *cache = Some(frag_bytes);
                    }

                    let body = cache.as_ref().unwrap();
                    let start = span_offset as usize;
                    let end = start + to_xfer;
                    buf[written..written + to_xfer].copy_from_slice(&body[start..end]);
                    self.pos += to_xfer as u64;
                    written += to_xfer;
                }
                FragmentKind::Unchanged { parent_offset } => {
                    let absolute_parent_offset = *parent_offset + span_offset;
                    self.parent
                        .seek(std::io::SeekFrom::Start(absolute_parent_offset))?;
                    let n = self.parent.read(&mut buf[written..written + to_xfer])?;
                    self.pos += n as u64;
                    written += n;
                    if n == 0 {
                        break;
                    }
                }
            }
        }

        Ok(written)
    }
}

impl std::io::Seek for RepoSeekableFile {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let new_pos: i128 = match pos {
            std::io::SeekFrom::Start(o) => o as i128,
            std::io::SeekFrom::End(o) => self.total_len as i128 + o as i128,
            std::io::SeekFrom::Current(o) => self.pos as i128 + o as i128,
        };

        if new_pos < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before start",
            ));
        }

        let capped = std::cmp::min(new_pos as u64, self.total_len);
        self.pos = capped;
        Ok(self.pos)
    }
}

fn get_path_in_metadata(path: &str) -> PathBuf {
    let mut p = PathBuf::new();
    p.push(REPO_METADATA_DIR_NAME);
    p.push(path);
    return p;
}
