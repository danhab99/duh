use crate::{
    diff::DiffFragment,
    hash::Hash,
    objects::{CommitStruct, FileFragment, FileVersion, Fragment, Object, ObjectReference, Person},
    replay::LazyFileReplay,
    utils::{self, find_file, getRepoConfigFileName, REPO_METADATA_DIR_NAME},
};
use crate::vlog;
use std::{
    collections::HashMap,
    error::Error,
    fs::{self, File},
    io::{Read, Seek, Write},
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
    me: Person,
    index: HashMap<String, Hash>,
}

pub type RepoError = Box<dyn Error>;
pub type RepoResult<T> = Result<T, RepoError>;

pub const BLOCK_SIZE: usize = 32;

/// Maximum size (bytes) for a single stored ADDED fragment. Larger ADDED
/// bodies are split into multiple Fragment objects at stage time.
pub const MAX_FRAGMENT_SIZE: usize = 10 * 1024 * 1024; // 10 MiB

impl Repo {
    pub fn at_root_path(root_path: Option<String>) -> RepoResult<Repo> {
        vlog!("repo::at_root_path called with root_path={:?}", root_path);
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

        // let buffer_size = config
        //     .get("chunk_size")
        //     .ok_or("missing chunk_size")?
        //     .as_integer()
        //     .ok_or("chunk_size is not a number")? as usize;

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
            // buffer_size,
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

        // Verbose: show resolved repo/user info
        vlog!("repo::at_root_path: user='{} <{}>' repo_root='{}'", r.me.name, r.me.email, r.root_path);

        let index_file_path = r.get_path_in_repo("index");
        vlog!("repo::at_root_path: index path = {}", index_file_path.display());

        let mut index_file = fs::File::open(index_file_path).unwrap();
        let mut contents = String::new();
        index_file.read_to_string(&mut contents)?;
        for line in contents.lines() {
            let parts = line.split("=").collect::<Vec<_>>();
            assert!(parts.len() == 2);

            // Trim whitespace and strip surrounding quotes so callers can write
            // either TOML-quoted keys/values or the simple `key = "hash"` form.
            let filepath_part = parts[0].trim().trim_matches('"');
            let hash_part = parts[1].trim().trim_matches('"');

            r.index.insert(
                filepath_part.to_string(),
                Hash::from_string(hash_part.to_string())?,
            );
        }

        vlog!("repo::at_root_path: loaded {} index entries", r.index.len());

        Ok(r)
    }

    fn get_path_in_repo(&self, p: &str) -> PathBuf {
        vlog!("repo::get_path_in_repo: p='{}'", p);
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
        vlog!("repo::get_path_in_cwd: p='{}' cwd={}", p, utils::get_cwd());
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
        vlog!("repo::initialize_at: root_path='{}'", root_path);
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

    pub fn get_object_path(&self, r: ObjectReference) -> RepoResult<PathBuf> {
        let hash = self.resolve_ref_name(r)?.to_string();
        vlog!("repo::get_object_path: resolved hash={}", hash);
        let top = &hash[0..2];
        let bottom = &hash[2..hash.len()];

        Ok(self.get_path_in_repo(format!("objects/{}/{}", top, bottom).as_str()))
    }

    pub fn save_obj(&self, o: Object) -> RepoResult<Hash> {
        vlog!("repo::save_obj: saving object");
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
        let hash = Hash::from_string(hex)?;

        // Persist temp file to final object path
        let path = self.get_object_path(ObjectReference::Hash(hash.clone()))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        tmp.persist(&path)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        vlog!("repo::save_obj: persisted object at {:?}", path);
        vlog!("repo::save_obj: object hash={}", hash.to_string());

        // NOTE: `.raw` sidecars removed — fragment bytes are stored only inside the
        // serialized `Fragment` object on disk and will be deserialized when first read.
        Ok(hash)
    }

    fn read_object(&self, r: Hash) -> RepoResult<Option<Vec<u8>>> {
        let path = self.get_object_path(ObjectReference::Hash(r))?;
        vlog!("repo::read_object: path={:?}", path);
        if !path.exists() {
            vlog!(
                "repo::read_object: object not found for hash={}",
                r.to_string()
            );
            return Ok(None);
        }
        let content = fs::read(path)?;
        vlog!("repo::read_object: read {} bytes", content.len());
        return Ok(Some(content.to_vec()));
    }

    pub fn get_object(&self, r: Hash) -> RepoResult<Option<Object>> {
        vlog!("repo::get_object: hash={}", r.to_string());
        let content = self.read_object(r)?;
        match content {
            Some(content) => {
                vlog!(
                    "repo::get_object: deserializing object ({} bytes)",
                    content.len()
                );
                Ok(Some(Object::from_msgpack(content)?))
            }
            None => {
                vlog!("repo::get_object: object not found");
                Ok(None)
            }
        }
    }

    fn get_ref_path(&self, name: &str) -> PathBuf {
        vlog!("repo::get_ref_path: name='{}'", name);
        return self.get_path_in_repo(format!("refs/{}", name).as_str());
    }

    pub fn set_ref(&self, name: &str, r: ObjectReference) -> RepoResult<()> {
        let path = self.get_ref_path(name);
        fs::write(path, r.to_string())?;
        vlog!("repo::set_ref: {} -> {}", name, r.to_string());
        return Ok(());
    }

    pub fn get_ref(&self, name: String) -> RepoResult<ObjectReference> {
        let ref_path = self.get_path_in_repo(format!("refs/{}", name).as_str());
        let val = String::from_utf8(fs::read(ref_path)?)?;
        vlog!("repo::get_ref: name='{}' content='{}'", name, val.trim());

        // Treat an empty ref file as "no parent" (map to zero hash) to make the
        // first commit/HEAD behaviour safe.
        if val.trim().is_empty() {
            vlog!("repo::get_ref: empty ref -> returning zero-hash");
            return Ok(ObjectReference::Hash(Hash::new()));
        }

        Ok(ObjectReference::from(val))
    }

    pub fn resolve_ref_name(&self, ref_name: ObjectReference) -> RepoResult<Hash> {
        match ref_name {
            ObjectReference::Hash(h) => {
                vlog!("repo::resolve_ref_name: given hash {}", h.to_string());
                Ok(h)
            }
            ObjectReference::Ref(r) => {
                vlog!("repo::resolve_ref_name: resolving ref '{}'", r);
                let n = self.get_ref(r.clone())?;
                let resolved = self.resolve_ref_name(n)?;
                vlog!(
                    "repo::resolve_ref_name: ref '{}' -> {}",
                    r,
                    resolved.to_string()
                );
                return Ok(resolved);
            }
        }
    }

    pub fn stage_file(&mut self, file_path: String) -> RepoResult<Hash> {
        let fp = self.get_path_in_cwd_str(&file_path);

        let mut new = File::open(fp.clone())?;

        let content_hash = Hash::digest_file_stream(&mut new)?;

        let head_commit_hash = self.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;

        let old: Box<dyn ReadSeek> = if head_commit_hash.is_zero() {
            Box::new(io::Cursor::new(Vec::new()))
        } else {
            // Load the previous file version into memory for diffing
            let mut reader = self.open_file(fp.clone(), head_commit_hash)?;
            let mut old_data = Vec::new();
            reader.read_to_end(&mut old_data)?;
            Box::new(io::Cursor::new(old_data))
        };

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
                Ok(DiffFragment::UNCHANGED { len }) => {
                    vlog!("repo::stage_file: UNCHANGED len={}", len);
                    file_fragments.push(FileFragment::UNCHANGED { len });
                }
                Ok(DiffFragment::DELETED { len }) => {
                    vlog!("repo::stage_file: DELETED len={}", len);
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
        vlog!(
            "repo::stage_file: indexed '{}' -> {}",
            fp,
            version_hash.to_string()
        );

        Ok(version_hash)
    }

    pub fn commit(&mut self, message: String) -> RepoResult<Hash> {
        vlog!("repo::commit: message='{}'", message);
        let head_commit = self.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?;

        let commit = CommitStruct {
            parent: head_commit,
            message: message,
            comitter: self.me.clone(),
            author: self.me.clone(),
            files: self.index.clone(),
        };

        vlog!("repo::commit: creating commit with {} files", commit.files.len());

        let commit_hash = self.save_obj(Object::Commit(commit))?;

        self.set_ref("HEAD", ObjectReference::Hash(commit_hash))?;
        vlog!("repo::commit: new HEAD = {}", commit_hash.to_string());

        Ok(commit_hash)
    }

    pub fn open_file(&mut self, file_path: String, hash: Hash) -> RepoResult<Box<dyn ReadSeek>> {
        vlog!(
            "repo::open_file: file_path='{}' hash={}",
            file_path,
            hash.to_string()
        );
        // Resolve the commit pointed to by `hash`; if it doesn't exist, return an empty reader.
        let fp = self.get_path_in_cwd_str(&file_path);

        let commit = match self.get_object(hash)? {
            Some(Object::Commit(c)) => c,
            None => return Ok(Box::new(io::Cursor::new(Vec::<u8>::new()))),
            _ => panic!("expected commit object"),
        };

        // Get file version hash for this path
        let file_version_hash = match commit.files.get(fp.as_str()) {
            Some(x) => x,
            None => {
                // File doesn't exist in this commit - return empty reader
                vlog!(
                    "repo::open_file: file '{}' not in commit {}",
                    fp,
                    hash.to_string()
                );
                return Ok(Box::new(io::Cursor::new(Vec::<u8>::new())));
            }
        };

        let file_version = match self.get_object(*file_version_hash)? {
            Some(Object::FileVersion(c)) => c,
            _ => panic!("expected file version"),
        };

        // Get parent file as a reader (may be lazy or empty)
        let parent_reader: Box<dyn ReadSeek> = if commit.parent.is_zero() {
            // No parent - use empty reader
            Box::new(io::Cursor::new(Vec::<u8>::new()))
        } else {
            // Recursively open parent file (this will also use LazyFileReplay)
            self.open_file(fp.clone(), commit.parent)?
        };

        vlog!(
            "repo::open_file: creating lazy replay with {} fragments, parent_hash={}",
            file_version.fragments.len(),
            commit.parent.to_string()
        );

        // Use LazyFileReplay to lazily reconstruct the file
        let replay = LazyFileReplay::new(self, parent_reader, file_version.fragments)?;

        Ok(Box::new(replay))
    }

    pub fn save_index(&mut self) -> RepoResult<()> {
        vlog!("repo::save_index: saving {} entries", self.index.len());
        let index_bytes = toml::to_string(&self.index)?;
        fs::write(self.get_path_in_repo("index"), index_bytes)?;

        let mut index_file = File::create(self.get_path_in_repo("index"))?;

        for (key, value) in &self.index {
            writeln!(index_file, "{} = \"{}\"", key, value.to_string())?;
        }

        Ok(())
    }
}

fn get_path_in_metadata(path: &str) -> PathBuf {
    vlog!("repo::get_path_in_metadata: path='{}'", path);
    let mut p = PathBuf::new();
    p.push(REPO_METADATA_DIR_NAME);
    p.push(path);
    return p;
}
