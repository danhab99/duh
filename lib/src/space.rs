use crate::{
    error::DuhError,
    hash::Hash,
    objects::{CommitStruct, Object, ObjectReference, Person, TreeEntry, TreeStruct},
    utils::{self},
    vlog,
};
use std::{
    collections::HashMap,
    error::Error,
    fs::File,
    io::{Read, Seek, Write},
    path::PathBuf,
    str::FromStr,
    time::SystemTime,
};

use opendal::blocking::Operator;
use serde::Serialize;

use toml::{self};

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

        let mut content = String::new();
        filesystem
            .read("config.toml")?
            .read_to_string(&mut content)?;
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

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let mut r = Space {
            fs: filesystem,
            chunk_size,
            max_size,
            worktree,
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
        let path = Path::new(path);
        if path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                self.create_dir_all(parent.to_str().unwrap())?;
            }
        }
        self.fs.create_dir(path.to_str().unwrap())?;

        Ok(())
    }

    fn get_path_in_space(&self, p: &str) -> PathBuf {
        vlog!("space::get_path_in_space: p='{}'", p);
        // returns `${root_path}/.duh/<p>` and ensures the metadata dir exists
        self.create_dir_all(p).unwrap();
        return PathBuf::from(p);
    }

    fn get_path_in_space_str(&self, p: &str) -> String {
        let b = self.get_path_in_space(p);
        let s = b.into_os_string().into_string().unwrap();
        return s;
    }

    // fn get_path_in_space_str(&self, p: &str) -> String {
    //     let b = self.get_path_in_space(p);
    //     let s = b.into_os_string().into_string().unwrap();
    //     return s;
    // }

    pub fn get_path_in_worktree(&self, p: &str) -> SpaceResult<PathBuf> {
        vlog!("space::get_path_in_worktree: p='{}'", p);
        match &self.worktree {
            Some(wt) => Ok(wt.join(p)),
            None => Err(Box::new(DuhError::BareRepoNoWorktree)),
        }
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

        let r = Space {
            fs: filesystem,
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
            "chunk_size = {}\nmax_size = {}\n\n[user]\nname = \"test\"\nemail = \"test@example.com\"\n",
            DEFAULT_BLOCK_SIZE, DEFAULT_MAX_FRAGMENT_SIZE
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
            self.create_dir_all(parent.as_os_str().to_str().unwrap())?;
        }
        tmp.persist(&path)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        vlog!("space::save_obj: persisted object at {:?}", path);
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
        let path = self.get_object_path(ObjectReference::Hash(r))?;
        vlog!("space::read_object: path={:?}", path);
        if !path.exists() {
            vlog!(
                "space::read_object: object not found for hash={}",
                r.to_string()
            );
            return Ok(None);
        }
        let mut content = Vec::<u8>::new();
        self.fs
            .reader(path.as_os_str().to_str().unwrap())?
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
                let n = self.get_ref(r.clone())?;
                let resolved = self.resolve_ref_name(n)?;
                vlog!(
                    "space::resolve_ref_name: ref '{}' -> {}",
                    r,
                    resolved.to_string()
                );
                return Ok(resolved);
            }
            ObjectReference::AbbrevHash(a) => {
                let r = self.get_ref(a.clone())?;
                return self.resolve_ref_name(r);
            }
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

    pub fn create_branch(&mut self, name: &str) -> SpaceResult<()> {
        let head_hash = self.get_head_commit_hash()?;
        self.set_ref(
            format!("head/{}", name).as_str(),
            ObjectReference::Hash(head_hash),
            Some(format!("create branch: {}", name).as_str()),
        )
    }

    pub fn list_refs(&mut self, path: &str) -> SpaceResult<Vec<ObjectReference>> {
        let refs_path = self.get_ref_path(path);

        let refs_names = self
            .fs
            .list(&refs_path)?
            .iter()
            .map(|x| ObjectReference::from_str(x.name()).unwrap())
            .collect::<Vec<_>>();

        Ok(refs_names)
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

        let fs = Operator::from_uri((url_s, config_pairs))?;
        let r: Space = Space::at_root_path(fs, None)?;
        Ok(r)
    }

    pub fn log_ref(&self, r: ObjectReference, hash: Hash, message: &str) -> SpaceResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.get_path_in_space(&format!("reflog/{}", r)))?;

        file.write_all(format!("{} | {}", hash.to_string(), message).as_bytes())?;

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
