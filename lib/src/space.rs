use crate::{
    error::DuhError,
    hash::Hash,
    objects::{CommitStruct, Object, ObjectReference, Person, TreeEntry, TreeStruct},
    vlog,
};
use std::{
    collections::HashMap,
    error::Error,
    fs::File,
    io::{Read, Seek, Write},
    path::{Component, Path, PathBuf},
    str::FromStr,
    time::SystemTime,
};

use opendal::BlockingOperator as Operator;
use serde::Serialize;

use toml::{self};

/// Resolve `.` and `..` components without touching the filesystem.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

fn upsert_pair(pairs: &mut Vec<(String, String)>, key: &str, value: String) {
    if let Some((_, v)) = pairs.iter_mut().find(|(k, _)| k == key) {
        *v = value;
    } else {
        pairs.push((key.to_string(), value));
    }
}

/// Opendal fs config for a local repo given either its worktree path
/// (containing `.duh/`) or its metadata directory.
fn local_fs_pairs(uri: &str, abs_path: PathBuf) -> SpaceResult<(opendal::Scheme, Vec<(String, String)>)> {
    let metadata_root = if abs_path
        .join(crate::utils::SPACE_METADATA_DIR_NAME)
        .is_dir()
    {
        abs_path.join(crate::utils::SPACE_METADATA_DIR_NAME)
    } else {
        abs_path
    };

    if !metadata_root.join("config").is_file() {
        return Err(Box::new(DuhError::Generic(format!(
            "remote path '{}' is not a valid duh repo (missing config)",
            uri
        ))));
    }

    let mut pairs: Vec<(String, String)> = Vec::new();
    upsert_pair(
        &mut pairs,
        "root",
        metadata_root.to_string_lossy().to_string(),
    );
    Ok((opendal::Scheme::Fs, pairs))
}

fn parse_uri_to_config_pairs(uri: &str) -> SpaceResult<(opendal::Scheme, Vec<(String, String)>)> {
    // Fallback for local path remotes like `../source` that are not valid URLs.
    let parsed = match url::Url::parse(uri) {
        Ok(u) if u.scheme() == "file" => {
            let path = u
                .to_file_path()
                .map_err(|_| format!("invalid file URL '{}'", uri))?;
            return local_fs_pairs(uri, path);
        }
        Ok(u) => u,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let raw_path = std::path::PathBuf::from(uri);
            let abs_path = if raw_path.is_absolute() {
                raw_path
            } else {
                std::env::current_dir()?.join(raw_path)
            };
            return local_fs_pairs(uri, abs_path);
        }
        Err(e) => return Err(Box::new(e)),
    };

    let scheme = opendal::Scheme::from_str(parsed.scheme())?;

    let mut pairs: Vec<(String, String)> = Vec::new();

    if let Some(host) = parsed.host_str() {
        let mut endpoint = format!("{}://{}", parsed.scheme(), host);
        if let Some(port) = parsed.port() {
            endpoint.push(':');
            endpoint.push_str(port.to_string().as_str());
        }

        upsert_pair(&mut pairs, "endpoint", endpoint);
        upsert_pair(&mut pairs, "host", host.to_string());

        // Common URI shape for s3 is s3://bucket/root
        if matches!(scheme, opendal::Scheme::S3) {
            upsert_pair(&mut pairs, "bucket", host.to_string());
        }
    }

    if !parsed.username().is_empty() {
        upsert_pair(&mut pairs, "username", parsed.username().to_string());
    }
    if let Some(password) = parsed.password() {
        upsert_pair(&mut pairs, "password", password.to_string());
    }

    let root = parsed.path();
    if !root.is_empty() && root != "/" {
        upsert_pair(&mut pairs, "root", root.to_string());
    }

    for (k, v) in parsed.query_pairs() {
        upsert_pair(&mut pairs, k.as_ref(), v.into_owned());
    }

    Ok((scheme, pairs))
}

// Object-safe alias for `Read + Seek` so we can store boxed readers that support both.
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek + ?Sized> ReadSeek for T {}

#[derive(Clone)]
pub struct Space {
    pub me: Person,
    pub index: HashMap<String, Hash>,
    pub chunk_size: usize,
    pub max_size: usize,
    pub worktree: Option<PathBuf>,
    pub metadata_root: PathBuf,
    fs: Operator,
}

pub type SpaceError = Box<dyn Error>;
pub type SpaceResult<T> = Result<T, SpaceError>;

pub const DEFAULT_BLOCK_SIZE: usize = 4096;

/// Maximum size (bytes) for a single stored ADDED fragment. Larger ADDED
/// bodies are split into multiple Fragment objects at stage time.
pub const DEFAULT_MAX_FRAGMENT_SIZE: usize = 10 * 1024 * 1024; // 10 MiB

impl Space {
    pub fn at_root_path(filesystem: Operator, worktree: Option<PathBuf>) -> SpaceResult<Space> {
        vlog!("space::at_root_path called with");
        // metadata_path == "<space-root>/.duh" — store the space root (parent of .duh)
        // let space_root = PathBuf::from(&metadata_path)
        //     .parent()
        //     .and_then(|p| p.to_str())
        //     .ok_or("couldn't determine space root")?
        //     .to_string();

        // let config_path = find_file(rp.as_str(), &getSpaceConfigFileName())?;

        let content = String::from_utf8(filesystem.read("config")?.to_bytes().to_vec())?;
        let config = content.parse::<toml::Table>()?;

        let user_config = config
            .get("user")
            .ok_or("missing user config")?
            .as_table()
            .ok_or("user config isn't a table")?;

        let chunk_size = config
            .get("chunk_size")
            .ok_or("missing chunk_size")?
            .as_integer()
            .ok_or("chunk_size is not a number")? as usize;

        let max_size = config
            .get("max_size")
            .ok_or("missing chunk_size")?
            .as_integer()
            .ok_or("chunk_size is not a number")? as usize;

        let config_bare = config
            .get("core")
            .and_then(|c| c.as_table())
            .and_then(|c| c.get("bare"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let metadata_root = PathBuf::from(filesystem.info().root());
        let mut r = Space {
            fs: filesystem,
            metadata_root,
            chunk_size,
            max_size,
            worktree: if config_bare { None } else { worktree },
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

        // Verbose: show resolved space/user info
        vlog!("space::at_root_path: user='{} <{}>'", r.me.name, r.me.email,);

        let index_file_path = r.get_path_in_space_str("index");
        vlog!("space::at_root_path: index path = {}", index_file_path,);

        let index_file = r.fs.reader(index_file_path.as_str()).unwrap();
        let mut contents = String::new();
        index_file
            .into_std_read(0..)?
            .read_to_string(&mut contents)?;
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

        vlog!(
            "space::at_root_path: loaded {} index entries",
            r.index.len()
        );

        Ok(r)
    }

    fn create_dir_all(&self, path: &str) -> Result<(), Box<dyn Error>> {
        use std::path::Path;
        if self.metadata_root.join(path).exists() {
            return Ok(());
        }
        if let Some(parent) = Path::new(path).parent() {
            let parent_str = parent.to_str().unwrap();
            if !parent_str.is_empty() {
                self.create_dir_all(parent_str)?;
            }
        }
        let mut dir_path = path.to_string();
        if !dir_path.ends_with('/') {
            dir_path.push('/');
        }
        self.fs.create_dir(dir_path.as_str())?;
        Ok(())
    }

    fn get_path_in_space(&self, p: &str) -> PathBuf {
        vlog!("space::get_path_in_space: p='{}'", p);
        // Ensure parent directories exist then return the absolute path.
        if let Some(parent) = PathBuf::from(p).parent() {
            let parent_str = parent.to_str().unwrap();
            if !parent_str.is_empty() {
                self.create_dir_all(parent_str).unwrap();
            }
        }
        self.metadata_root.join(p)
    }

    fn get_path_in_space_str(&self, p: &str) -> String {
        // Ensure the directory structure exists (side-effect of get_path_in_space),
        // then return the relative path string for OpenDAL.
        let _ = self.get_path_in_space(p);
        p.to_string()
    }

    pub fn get_path_in_worktree(&self, p: &str) -> SpaceResult<PathBuf> {
        vlog!("space::get_path_in_worktree: p='{}'", p);
        match &self.worktree {
            Some(wt) => Ok(wt.join(p)),
            None => Err(Box::new(DuhError::BareRepoNoWorktree)),
        }
    }

    /// Convert a path given by the user (absolute, or relative to `cwd`) into a
    /// '/'-separated path relative to the worktree root, as stored in the index.
    pub fn repo_path_from(&self, cwd: &Path, input: &str) -> SpaceResult<String> {
        let worktree = self
            .worktree
            .as_ref()
            .ok_or_else(|| Box::new(DuhError::BareRepoNoWorktree) as Box<dyn Error>)?;

        let abs = lexical_normalize(&cwd.join(input));
        let rel = match abs.strip_prefix(lexical_normalize(worktree)) {
            Ok(rel) => rel.to_path_buf(),
            // Fall back to resolving symlinks when the path exists.
            Err(_) => std::fs::canonicalize(&abs)
                .ok()
                .zip(std::fs::canonicalize(worktree).ok())
                .and_then(|(a, w)| a.strip_prefix(w).ok().map(Path::to_path_buf))
                .ok_or_else(|| format!("path '{}' is outside the worktree", input))?,
        };

        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        if parts.is_empty() {
            return Err(format!("path '{}' is the worktree root, not a file", input).into());
        }
        if parts[0] == crate::utils::SPACE_METADATA_DIR_NAME {
            return Err(format!("path '{}' is inside the metadata directory", input).into());
        }
        Ok(parts.join("/"))
    }

    /// Render a repo path relative to `cwd` for display (e.g. `../a.bin`).
    pub fn display_path(&self, cwd: &Path, repo_path: &str) -> String {
        let Some(worktree) = &self.worktree else {
            return repo_path.to_string();
        };
        let target = lexical_normalize(&worktree.join(repo_path));
        let cwd = lexical_normalize(cwd);

        let common = target
            .components()
            .zip(cwd.components())
            .take_while(|(a, b)| a == b)
            .count();
        let mut out = PathBuf::new();
        for _ in cwd.components().skip(common) {
            out.push("..");
        }
        for c in target.components().skip(common) {
            out.push(c);
        }
        out.to_string_lossy().to_string()
    }

    pub fn get_path_in_worktree_str(&self, p: &str) -> SpaceResult<String> {
        let b = self.get_path_in_worktree(p)?;
        let s = b.into_os_string().into_string().unwrap();
        Ok(s)
    }

    /// Return a list of file paths currently present in the index (cloned).
    pub fn index_paths(&self) -> Vec<String> {
        self.index.keys().cloned().collect()
    }

    /// Return the stored FileVersion *object* hash for `path` if present in the index.
    pub fn get_indexed_version(&self, path: &str) -> Option<Hash> {
        self.index.get(path).cloned()
    }

    pub fn initialize_at(filesystem: Operator, worktree: Option<PathBuf>) -> SpaceResult<Space> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let metadata_root = PathBuf::from(filesystem.info().root());
        let r = Space {
            fs: filesystem,
            metadata_root,
            chunk_size: DEFAULT_BLOCK_SIZE,
            max_size: DEFAULT_MAX_FRAGMENT_SIZE,
            worktree,
            me: Person {
                name: "test".to_string(),
                email: "test@example.com".to_string(),
                timestamp: now,
            },
            index: HashMap::new(),
        };

        vlog!("space::initialize_at");
        r.create_dir_all("objects")?;
        r.create_dir_all("refs")?;

        let default_config = format!(
            "chunk_size = {}\nmax_size = {}\n\n[core]\nbare = {}\n\n[user]\nname = \"test\"\nemail = \"test@example.com\"\n",
            DEFAULT_BLOCK_SIZE,
            DEFAULT_MAX_FRAGMENT_SIZE,
            r.worktree.is_none()
        );

        r.write_to_space(
            r.get_path_in_space_str("config").as_str(),
            default_config.as_bytes(),
        )?;
        r.write_to_space(r.get_path_in_space_str("index").as_str(), b"")?;

        // refs/main starts empty (interpreted as zero hash); HEAD points to main.
        r.write_to_space(r.get_ref_path("main").as_str(), b"")?;
        r.write_to_space(r.get_ref_path("HEAD").as_str(), b"main")?;

        Ok(r)
    }

    pub fn get_object_path(&self, r: ObjectReference) -> SpaceResult<PathBuf> {
        let hash = self.resolve_ref_name(r)?.to_string();
        vlog!("space::get_object_path: resolved hash={}", hash);
        let top = &hash[0..3];
        let top1 = &hash[3..6];
        let top2 = &hash[6..9];
        let bottom = &hash[9..hash.len()];

        Ok(
            self.get_path_in_space(
                format!("objects/{}/{}/{}/{}", top, top1, top2, bottom).as_str(),
            ),
        )
    }

    pub fn save_obj(&self, o: Object) -> SpaceResult<Hash> {
        vlog!("space::save_obj: saving object");
        use rmp_serde::Serializer;
        use sha2::{Digest, Sha256};
        use std::io::Write as IoWrite;
        use tempfile::NamedTempFile;

        // Ensure objects dir exists
        let objects_dir = &self.get_path_in_space_str("objects");
        self.create_dir_all(objects_dir)?;

        // Create a named temp file inside the objects dir so rename/persist is atomic
        let mut tmp = NamedTempFile::new_in(self.metadata_root.join(&objects_dir))?;

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
        // get_object_path returns an absolute path; strip root to get the
        // relative path needed by create_dir_all (OpenDAL) while using the
        // absolute path for the native-FS persist call.
        let abs_path = self.get_object_path(ObjectReference::Hash(hash.clone()))?;
        let rel_path = abs_path.strip_prefix(&self.metadata_root).unwrap();
        if let Some(parent) = rel_path.parent() {
            let parent_str = parent.to_str().unwrap();
            if !parent_str.is_empty() {
                self.create_dir_all(parent_str)?;
            }
        }
        tmp.persist(&abs_path)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        vlog!("space::save_obj: persisted object at {:?}", abs_path);
        vlog!("space::save_obj: object hash={}", hash.to_string());

        // NOTE: `.raw` sidecars removed — fragment bytes are stored only inside the
        // serialized `Fragment` object on disk and will be deserialized when first read.
        Ok(hash)
    }

    fn read_in_space(&self, path: &str) -> SpaceResult<Option<Vec<u8>>> {
        let mut content = Vec::<u8>::new();
        self.fs
            .reader(path)?
            .into_std_read(0..)?
            .read_to_end(&mut content)?;

        return Ok(Some(content));
    }

    fn read_object(&self, r: Hash) -> SpaceResult<Option<Vec<u8>>> {
        let abs_path = self.get_object_path(ObjectReference::Hash(r))?;
        let rel_path = abs_path.strip_prefix(&self.metadata_root).unwrap();
        vlog!("space::read_object: path={:?}", abs_path);
        if !abs_path.exists() {
            vlog!(
                "space::read_object: object not found for hash={}",
                r.to_string()
            );
            return Ok(None);
        }
        let mut content = Vec::<u8>::new();
        self.fs
            .reader(rel_path.to_str().unwrap())?
            .into_std_read(0..)?
            .read_to_end(&mut content)?;
        vlog!("space::read_object: read {} bytes", content.len());
        return Ok(Some(content.to_vec()));
    }

    pub fn get_object(&self, r: Hash) -> SpaceResult<Option<Object>> {
        vlog!("space::get_object: hash={}", r.to_string());
        let content = self.read_object(r)?;
        match content {
            Some(content) => {
                vlog!(
                    "space::get_object: deserializing object ({} bytes)",
                    content.len()
                );
                Ok(Some(Object::from_msgpack(content)?))
            }
            None => {
                vlog!("space::get_object: object not found");
                Ok(None)
            }
        }
    }

    pub fn get_ref_path(&self, name: &str) -> String {
        vlog!("space::get_ref_path: name='{}'", name);
        return self.get_path_in_space_str(format!("refs/{}", name).as_str());
    }

    pub fn write_to_space(&self, path: &str, content: &[u8]) -> SpaceResult<()> {
        self.fs.write(path, Vec::from(content))?;
        Ok(())
    }

    pub fn set_ref(
        &self,
        name: &str,
        r: ObjectReference,
        log_msg: Option<&str>,
    ) -> SpaceResult<()> {
        let path = self.get_ref_path(name);
        self.write_to_space(path.as_str(), r.to_string().as_bytes())?;
        vlog!("space::set_ref: {} -> {}", name, r.to_string());

        if let Some(log_msg) = log_msg {
            let _ = self.log_ref(
                ObjectReference::Ref(name.to_string()),
                self.resolve_ref_name(r)?,
                log_msg,
            );
        }

        return Ok(());
    }

    pub fn get_ref(&self, name: String) -> SpaceResult<ObjectReference> {
        let ref_path = &self.get_path_in_space_str(format!("refs/{}", name).as_str());
        let mut val = String::new();
        self.fs
            .reader(ref_path)?
            .into_std_read(0..)?
            .read_to_string(&mut val)?;
        vlog!("space::get_ref: name='{}' content='{}'", name, val.trim());

        // Treat an empty ref file as "no parent" (map to zero hash) to make the
        // first commit/HEAD behaviour safe.
        if val.trim().is_empty() {
            vlog!("space::get_ref: empty ref -> returning zero-hash");
            return Ok(ObjectReference::Hash(Hash::new()));
        }

        Ok(ObjectReference::from(val))
    }

    pub fn resolve_ref_name(&self, ref_name: ObjectReference) -> SpaceResult<Hash> {
        match ref_name {
            ObjectReference::Hash(h) => {
                vlog!("space::resolve_ref_name: given hash {}", h.to_string());
                Ok(h)
            }
            ObjectReference::Ref(r) => {
                vlog!("space::resolve_ref_name: resolving ref '{}'", r);
                // Bare names resolve as branches first, then as tags.
                let n = match self.get_ref(r.clone()) {
                    Ok(n) => n,
                    Err(e) => match self.get_ref(format!("tags/{}", r)) {
                        Ok(n) => n,
                        Err(_) => return Err(e),
                    },
                };
                let resolved = self.resolve_ref_name(n)?;
                vlog!(
                    "space::resolve_ref_name: ref '{}' -> {}",
                    r,
                    resolved.to_string()
                );
                return Ok(resolved);
            }
            ObjectReference::AbbrevHash(a) => {
                // A hex-looking name may still be a branch or tag.
                if let Ok(h) = self.resolve_ref_name(ObjectReference::Ref(a.clone())) {
                    return Ok(h);
                }
                self.resolve_commit_prefix(&a)
            }
        }
    }

    /// The unique commit whose hash starts with `prefix` (at least 4 hex
    /// digits). Errors if none or several match.
    pub fn resolve_commit_prefix(&self, prefix: &str) -> SpaceResult<Hash> {
        let prefix = prefix.to_lowercase();
        if prefix.len() < 4 || prefix.len() > 64 || !prefix.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("'{}' is not a valid abbreviated hash", prefix).into());
        }

        // Objects live at objects/<3>/<3>/<3>/<rest>; walk only the
        // directories that can match the prefix.
        let segments = [(0usize, 3usize), (3, 6), (6, 9), (9, 64)];
        let mut dirs = vec![(self.metadata_root.join("objects"), String::new())];
        for (start, end) in segments {
            let want = prefix.get(start..end.min(prefix.len())).unwrap_or("");
            let mut next = Vec::new();
            for (dir, so_far) in dirs {
                let Ok(entries) = std::fs::read_dir(&dir) else { continue };
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.len() == end - start && name.starts_with(want) {
                        next.push((entry.path(), format!("{}{}", so_far, name)));
                    }
                }
            }
            dirs = next;
        }

        let mut matches = Vec::new();
        for (_, hex) in dirs {
            let h = Hash::from_string(hex)?;
            if let Some(Object::Commit(_)) = self.get_object(h)? {
                matches.push(h);
            }
        }
        match matches.as_slice() {
            [h] => Ok(*h),
            [] => Err(format!("no commit or ref matches '{}'", prefix).into()),
            _ => Err(format!("'{}' is ambiguous ({} commits match)", prefix, matches.len()).into()),
        }
    }

    pub fn save_index(&mut self) -> SpaceResult<()> {
        vlog!("space::save_index: saving {} entries", self.index.len());

        let mut index_file = File::create(self.get_path_in_space("index"))?;

        for (key, value) in &self.index {
            writeln!(index_file, "{} = \"{}\"", key, value.to_string())?;
        }

        Ok(())
    }

    pub fn list_files(&mut self, commit_ref: ObjectReference) -> SpaceResult<Vec<String>> {
        let commit_hash = self.resolve_ref_name(commit_ref)?;
        let files = self.get_commit_files(commit_hash)?;
        Ok(files.keys().cloned().collect())
    }

    pub fn get_head_commit_hash(&mut self) -> SpaceResult<Hash> {
        Ok(self.resolve_ref_name(ObjectReference::Ref("HEAD".to_string()))?)
    }

    /// Returns the branch name that HEAD points to (e.g. "main").
    pub fn get_head_branch_name(&self) -> SpaceResult<String> {
        let head_ref = self.get_ref("HEAD".to_string())?;
        match head_ref {
            ObjectReference::Ref(branch) => Ok(branch),
            _ => Err("HEAD does not point to a branch".into()),
        }
    }

    pub fn get_head_commit(&mut self) -> SpaceResult<CommitStruct> {
        let commit_hash = self.get_head_commit_hash()?;
        match self.get_object(commit_hash)? {
            Some(Object::Commit(commit)) => Ok(commit),
            _ => Err(Box::new(crate::error::DuhError::invalid_object(
                "commit",
                "unknown object type",
            ))),
        }
    }

    /// Build a tree object from a flat map of file paths to hashes, returning the tree's hash.
    pub fn build_tree(&self, files: &HashMap<String, Hash>) -> SpaceResult<Hash> {
        let mut entries: Vec<TreeEntry> = Vec::new();
        let mut dirs: HashMap<String, HashMap<String, Hash>> = HashMap::new();

        for (path, hash) in files {
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() == 1 {
                entries.push(TreeEntry {
                    name: path.clone(),
                    mode: 0o100644,
                    hash: *hash,
                });
            } else {
                let dir = parts[0].to_string();
                let subpath = parts[1..].join("/");
                dirs.entry(dir).or_default().insert(subpath, *hash);
            }
        }

        for (dir_name, sub_files) in dirs {
            let sub_tree_hash = self.build_tree(&sub_files)?;
            entries.push(TreeEntry {
                name: dir_name,
                mode: 0o40000,
                hash: sub_tree_hash,
            });
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let tree = TreeStruct { entries };
        self.save_obj(Object::Tree(tree))
    }

    /// Walk a tree object recursively, returning a flat map of file paths to hashes.
    pub fn walk_tree(&self, tree_hash: Hash) -> SpaceResult<HashMap<String, Hash>> {
        let mut files = HashMap::new();

        let tree = match self.get_object(tree_hash)? {
            Some(Object::Tree(t)) => t,
            None => return Ok(files),
            _ => {
                return Err(Box::new(crate::error::DuhError::invalid_object(
                    "tree",
                    "expected tree object",
                )))
            }
        };

        for entry in tree.entries {
            if entry.mode == 0o40000 {
                let sub_files = self.walk_tree(entry.hash)?;
                for (sub_path, hash) in sub_files {
                    files.insert(format!("{}/{}", entry.name, sub_path), hash);
                }
            } else {
                files.insert(entry.name, entry.hash);
            }
        }

        Ok(files)
    }

    /// Get the flat file map for a commit by walking its root tree.
    pub fn get_commit_files(&self, commit_hash: Hash) -> SpaceResult<HashMap<String, Hash>> {
        let commit = match self.get_object(commit_hash)? {
            Some(Object::Commit(c)) => c,
            _ => {
                return Err(Box::new(crate::error::DuhError::invalid_object(
                    "commit",
                    "expected commit object",
                )))
            }
        };
        self.walk_tree(commit.tree)
    }

    /// Names of the refs stored directly in `refs/<dir>` (`""` for top-level
    /// refs, i.e. branches and HEAD), sorted. Subdirectories are skipped.
    pub fn list_ref_names(&self, dir: &str) -> SpaceResult<Vec<String>> {
        let mut path = self.get_ref_path(dir);
        if !path.ends_with('/') {
            path.push('/');
        }
        if !self.fs.exists(&path)? {
            return Ok(Vec::new());
        }
        let mut names: Vec<String> = self
            .fs
            .list(&path)?
            .into_iter()
            .filter(|e| e.metadata().is_file())
            .map(|e| e.name().to_string())
            .collect();
        names.sort();
        Ok(names)
    }

    /// Branch names (top-level refs other than HEAD), sorted.
    pub fn list_branches(&self) -> SpaceResult<Vec<String>> {
        Ok(self
            .list_ref_names("")?
            .into_iter()
            .filter(|n| n != "HEAD")
            .collect())
    }

    pub fn delete_ref(&mut self, path: &str) -> SpaceResult<()> {
        let ref_path = self.get_ref_path(path);
        self.fs.delete(ref_path.as_str())?;
        Ok(())
    }

    fn get_table(&self) -> SpaceResult<toml::Table> {
        let content = String::from_utf8(self.read_in_space("config")?.unwrap())?;
        let table = toml::from_str(&content)?;

        return Ok(table);
    }

    /// Read a value from the config file by dot-separated key (e.g. `user.name`, `chunk_size`).
    pub fn get_config_value(&self, key: &str) -> SpaceResult<String> {
        let table = self.get_table()?;

        let parts: Vec<&str> = key.splitn(2, '.').collect();
        let value = if parts.len() == 2 {
            table
                .get(parts[0])
                .and_then(|v| v.as_table())
                .and_then(|t| t.get(parts[1]))
        } else {
            table.get(parts[0])
        };

        match value {
            Some(v) => Ok(v.to_string().trim_matches('"').to_string()),
            None => Err(format!("config key '{}' not found", key).into()),
        }
    }

    /// Write a value to the config file by dot-separated key (e.g. `user.name`, `chunk_size`).
    pub fn set_config_value(&self, key: &str, value: &str) -> SpaceResult<()> {
        let mut table = self.get_table()?;

        let parts: Vec<&str> = key.splitn(2, '.').collect();
        if parts.len() == 2 {
            let section = table
                .entry(parts[0])
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            let section_table = section
                .as_table_mut()
                .ok_or_else(|| format!("'{}' is not a table", parts[0]))?;
            section_table.insert(parts[1].to_string(), toml::Value::String(value.to_string()));
        } else {
            // Try to preserve integer type for known numeric keys.
            let parsed = if let Ok(n) = value.parse::<i64>() {
                toml::Value::Integer(n)
            } else if let Ok(b) = value.parse::<bool>() {
                toml::Value::Boolean(b)
            } else {
                toml::Value::String(value.to_string())
            };
            table.insert(parts[0].to_string(), parsed);
        }

        let config_path = self.get_path_in_space_str("config");

        self.write_to_space(&config_path, toml::to_string(&table)?.as_bytes())?;
        Ok(())
    }

    pub fn get_remote_by_name(&self, name: &str) -> SpaceResult<Space> {
        let full_table = self.get_table()?;

        let this_remote_table = full_table
            .get("remote")
            .and_then(|r| r.as_table())
            .and_then(|table| table.get(name)?.as_table())
            .ok_or(DuhError::RemoteNotFound(name.to_string()))?;

        let url_s = this_remote_table
            .get("url")
            .ok_or(DuhError::Generic("remote requires url".to_string()))?
            .as_str()
            .ok_or(DuhError::Generic("remote url must be a string".to_string()))?;

        let config_pairs: Vec<(String, String)> = this_remote_table
            .get("config")
            .and_then(|c| c.as_table())
            .map(|t| {
                t.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let (scheme, mut merged_pairs) = parse_uri_to_config_pairs(url_s)?;

        // Explicit remote.config values override URI-derived defaults.
        for (k, v) in config_pairs {
            upsert_pair(&mut merged_pairs, k.as_str(), v);
        }

        let fs = opendal::Operator::via_iter(scheme, merged_pairs)?.blocking();
        let r: Space = Space::at_root_path(fs, None)?;
        Ok(r)
    }

    pub fn log_ref(&self, r: ObjectReference, hash: Hash, message: &str) -> SpaceResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.get_path_in_space(&format!("reflog/{}", r)))?;

        writeln!(file, "{} | {}", hash.to_string(), message)?;

        Ok(())
    }

    pub fn get_reflog(&self, branch: &str) -> SpaceResult<String> {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .open(self.get_path_in_space(&format!("reflog/{}", branch)))?;

        let mut s: Vec<u8> = Vec::new();
        file.read_to_end(&mut s)?;

        Ok(String::from_utf8(s)?)
    }

    /// List all configured remotes as (name, url) pairs.
    pub fn list_remotes(&self) -> SpaceResult<Vec<(String, String)>> {
        let full_table = self.get_table()?;
        let remote_table = full_table.get("remote").and_then(|r| r.as_table());

        let mut result = Vec::new();
        if let Some(table) = remote_table {
            for (name, remote_val) in table {
                let url = remote_val
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                result.push((name.clone(), url));
            }
        }
        Ok(result)
    }

    /// Add a new remote with the given name and URL.
    pub fn add_remote(&self, name: &str, url: &str) -> SpaceResult<()> {
        let mut table = self.get_table()?;

        let remote_section = table
            .entry("remote")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let remote_table = remote_section
            .as_table_mut()
            .ok_or_else(|| "remote section is not a table".to_string())?;

        let remote_entry = remote_table
            .entry(name.to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let remote_entry_table = remote_entry
            .as_table_mut()
            .ok_or_else(|| format!("'{}' is not a table", name))?;

        remote_entry_table.insert(
            "url".to_string(),
            toml::Value::String(url.to_string()),
        );

        let config_path = self.get_path_in_space_str("config");
        self.write_to_space(&config_path, toml::to_string(&table)?.as_bytes())?;
        Ok(())
    }

    /// Remove a remote by name.
    pub fn remove_remote(&self, name: &str) -> SpaceResult<()> {
        let mut table = self.get_table()?;

        let remote_section = table
            .get_mut("remote")
            .and_then(|r| r.as_table_mut())
            .ok_or_else(|| "no remotes configured".to_string())?;

        if remote_section.remove(name).is_none() {
            return Err(DuhError::RemoteNotFound(name.to_string()).into());
        }

        let config_path = self.get_path_in_space_str("config");
        self.write_to_space(&config_path, toml::to_string(&table)?.as_bytes())?;
        Ok(())
    }

    /// Rename a remote from old_name to new_name.
    pub fn rename_remote(&self, old_name: &str, new_name: &str) -> SpaceResult<()> {
        let mut table = self.get_table()?;

        let remote_section = table
            .get_mut("remote")
            .and_then(|r| r.as_table_mut())
            .ok_or_else(|| "no remotes configured".to_string())?;

        let remote_entry = remote_section
            .remove(old_name)
            .ok_or_else(|| DuhError::RemoteNotFound(old_name.to_string()))?;

        remote_section.insert(new_name.to_string(), remote_entry);

        let config_path = self.get_path_in_space_str("config");
        self.write_to_space(&config_path, toml::to_string(&table)?.as_bytes())?;
        Ok(())
    }

    /// Get the URL for a remote.
    pub fn get_remote_url(&self, name: &str) -> SpaceResult<String> {
        let full_table = self.get_table()?;

        let url = full_table
            .get("remote")
            .and_then(|r| r.as_table())
            .and_then(|table| table.get(name))
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("url"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| DuhError::RemoteNotFound(name.to_string()))?;

        Ok(url.to_string())
    }

    /// Set the URL for a remote.
    pub fn set_remote_url(&self, name: &str, url: &str) -> SpaceResult<()> {
        let mut table = self.get_table()?;

        let remote_entry = table
            .get_mut("remote")
            .and_then(|r| r.as_table_mut())
            .and_then(|table| table.get_mut(name))
            .and_then(|v| v.as_table_mut())
            .ok_or_else(|| DuhError::RemoteNotFound(name.to_string()))?;

        remote_entry.insert(
            "url".to_string(),
            toml::Value::String(url.to_string()),
        );

        let config_path = self.get_path_in_space_str("config");
        self.write_to_space(&config_path, toml::to_string(&table)?.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// A fresh space whose worktree is a temp dir (metadata in `.duh/`).
    fn space_with_worktree() -> (TempDir, Space) {
        let dir = TempDir::new().unwrap();
        let meta = dir.path().join(crate::utils::SPACE_METADATA_DIR_NAME);
        std::fs::create_dir_all(&meta).unwrap();
        let op = opendal::services::Fs::default().root(meta.to_str().unwrap());
        let fs = opendal::Operator::new(op).unwrap().finish().blocking();
        let space = Space::initialize_at(fs, Some(dir.path().to_path_buf())).unwrap();
        (dir, space)
    }

    #[test]
    fn repo_path_from_resolves_against_cwd() {
        let (dir, space) = space_with_worktree();
        let wt = dir.path();
        let sub = wt.join("sub");

        assert_eq!(space.repo_path_from(wt, "a/b.bin").unwrap(), "a/b.bin");
        assert_eq!(space.repo_path_from(wt, "./a//b.bin").unwrap(), "a/b.bin");
        assert_eq!(space.repo_path_from(&sub, "x.bin").unwrap(), "sub/x.bin");
        assert_eq!(space.repo_path_from(&sub, "../y.bin").unwrap(), "y.bin");
        assert_eq!(space.repo_path_from(&sub, "deep/../z.bin").unwrap(), "sub/z.bin");

        let abs = wt.join("sub/abs.bin");
        assert_eq!(space.repo_path_from(&sub, abs.to_str().unwrap()).unwrap(), "sub/abs.bin");
    }

    #[test]
    fn repo_path_from_rejects_bad_paths() {
        let (dir, space) = space_with_worktree();
        let wt = dir.path();

        assert!(space.repo_path_from(wt, "../outside.bin").is_err());
        assert!(space.repo_path_from(wt, "/etc/hostname").is_err());
        assert!(space.repo_path_from(wt, ".").is_err());
        assert!(space.repo_path_from(wt, ".duh/config").is_err());
    }

    #[test]
    fn display_path_is_relative_to_cwd() {
        let (dir, space) = space_with_worktree();
        let wt = dir.path();

        assert_eq!(space.display_path(wt, "a/b.bin"), "a/b.bin");
        assert_eq!(space.display_path(&wt.join("a"), "a/b.bin"), "b.bin");
        assert_eq!(space.display_path(&wt.join("c"), "a/b.bin"), "../a/b.bin");
        assert_eq!(space.display_path(&wt.join("c/d"), "y.bin"), "../../y.bin");
    }
}
