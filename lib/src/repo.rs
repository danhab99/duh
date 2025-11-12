use crate::{
    diff::{self, DiffFragment},
    hash::Hash,
    objects::{
        CommitStruct, FileRefStruct, FileStruct, Fragment, Object, ObjectReference, Person, TreeRefStruct, TreeStruct
    },
    utils::{self, REPO_METADATA_DIR_NAME, find_file, getRepoConfigFileName},
};
use std::{env, error::Error, fs::{self, File}, io::Read, path::PathBuf, str::FromStr, time::SystemTime};

use toml;

pub struct Repo {
    root_path: String,
    buffer_size: usize,
    me: Person,
}

pub type RepoError = Box<dyn Error>;
pub type RepoResult<T> = Result<T, RepoError>;

const BLOCK_SIZE = 512;

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
            .as_float()
            .ok_or("chunk_size is not a number")? as usize;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let rp = find_file(rp.as_str(), REPO_METADATA_DIR_NAME)?;

        Ok(Repo {
            root_path: rp,
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
        })
    }

    fn get_path_in_repo(&self, p: &str) -> PathBuf {
        let mut b = PathBuf::from(self.root_path.clone()).join(p);
        b.push(utils::REPO_METADATA_DIR_NAME);
        fs::create_dir_all(p).unwrap();
        return b;
    }

    fn get_path_in_repo_str(&self, p: &str) -> String {
        let b = self.get_path_in_repo(p);
        let s = b.into_os_string().into_string().unwrap();
        return s;
    }

    fn get_path_in_cwd(&self, p: &str) -> PathBuf {
        PathBuf::from(self.root_path.clone())
            .join(utils::get_cwd())
            .join(p)
    }

    fn get_path_in_cwd_str(&self, p: &str) -> String {
        let b = self.get_path_in_cwd(p);
        let s = b.into_os_string().into_string().unwrap();
        return s;
    }

    pub fn initialize_at(root_path: String) -> RepoResult<Repo> {
        fs::create_dir_all(get_path_in_metadata("objects"))?;
        fs::create_dir_all(get_path_in_metadata("refs"))?;
        fs::write(get_path_in_metadata("config"), "# duh config")?;
        fs::write(get_path_in_metadata("HEAD"), "")?;

        Ok(Repo::at_root_path(Some(root_path))?)
    }

    fn get_object_path(&self, r: ObjectReference) -> RepoResult<PathBuf> {
        let hash = self.resolve_ref_name(r)?.to_string();
        let top = &hash[0..2];
        let bottom = &hash[2..hash.len()];

        Ok(self.get_path_in_repo(format!("objects/{}/{}", top, bottom).as_str()))
    }

    fn save_obj(&self, o: Object) -> RepoResult<Hash> {
        let (msgpack, hash) = o.hash()?;
        let path = self.get_object_path(ObjectReference::Hash(hash.clone()))?;
        fs::write(path, msgpack)?;
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

    fn get_object(&self, r: Hash) -> RepoResult<Option<Object>> {
        let content = self.read_object(r)?;
        match content {
            Some(content) => Ok(Some(Object::from_msgpack(content)?)),
            None => Ok(None),
        }
    }

    fn get_ref_path(&self, name: &str) -> PathBuf {
        return self.get_path_in_repo(format!("refs/{}", name).as_str());
    }

    fn set_ref(&self, name: &str, r: ObjectReference) -> RepoResult<()> {
        let path = self.get_ref_path(name);
        fs::write(path, r.to_string())?;
        return Ok(());
    }

    pub fn get_ref(&self, name: String) -> RepoResult<ObjectReference> {
        let ref_path = self.get_path_in_repo(format!("refs/{}", name).as_str());
        let val = String::from_utf8(fs::read(ref_path)?)?;
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

    pub fn get_commit_object(&self, r: ObjectReference) -> RepoResult<Option<CommitStruct>> {
        let hash = self.resolve_ref_name(r)?;
        let obj = self.get_object(hash)?.expect("commit not found");

        if let Object::Commit(commit) = obj {
            return Ok(Some(commit));
        } else {
            return Ok(None);
        }
    }

    pub fn build_file_struct(
        &self,
        content_hash: Hash,
        diff_fragments: &[DiffFragment],
    ) -> RepoResult<FileStruct> {
        let mut hashes = Vec::new();
        for frag in diff_fragments {
            let hash = self.save_obj(Object::Fragment(Fragment(*x)))?;
            hashes.push(hash);
        }

        let f = FileStruct {
            fragments: hashes,
            content_hash,
        };

        return Ok(f);
    }

    pub fn get_head_commit(&self) -> RepoResult<Option<CommitStruct>> {
        let r = ObjectReference::from_str("HEAD")?;
        let h = self.resolve_ref_name(r)?;
        self.get_commit_object(h)
    }

    /// Get the previous version of a file from the HEAD commit
    /// 
    /// # Arguments
    /// * `path` - The path to the file relative to the repository root
    /// 
    /// # Returns
    /// * `Ok(Some(FileStruct))` - If the file exists in the HEAD commit
    /// * `Ok(None)` - If there is no HEAD commit or the file doesn't exist in HEAD
    /// * `Err` - If there was an error accessing the repository
    fn get_previous_file_version(&self, path: &PathBuf) -> RepoResult<Option<FileStruct>> {
        // Get the HEAD commit
        let head = self.get_head_commit()?;
        
        if head.is_none() {
            return Ok(None);
        }
        
        let commit = head.unwrap();
        
        // Convert path to components for traversal
        let components: Vec<String> = path
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(String::from))
            .collect();
        
        if components.is_empty() {
            return Ok(None);
        }
        
        // Start with the root trees from the commit
        let mut current_tree_refs = &commit.trees;
        
        // Traverse the tree structure following the path components
        for (i, component) in components.iter().enumerate() {
            let is_last = i == components.len() - 1;
            
            // Look for a matching tree or file
            if is_last {
                // Last component - look for a file in the current tree level
                // We need to get the actual tree objects to access files
                for tree_ref in current_tree_refs {
                    if let Some(Object::Tree(tree)) = self.get_object(tree_ref.hash.clone())? {
                        // Look for the file in this tree
                        if let Some(file_ref) = tree.files.iter().find(|f| &f.name == component) {
                            // Found the file, now get the FileStruct
                            if let Some(Object::File(file_struct)) = self.get_object(file_ref.hash.clone())? {
                                return Ok(Some(file_struct));
                            }
                        }
                    }
                }
                // File not found in any tree at this level
                return Ok(None);
            } else {
                // Not the last component - look for a matching subtree
                let matching_tree = current_tree_refs.iter().find(|t| &t.name == component);
                
                if let Some(tree_ref) = matching_tree {
                    // Get the tree object
                    if let Some(Object::Tree(tree)) = self.get_object(tree_ref.hash.clone())? {
                        // Move to the subtrees of this tree
                        current_tree_refs = &tree.trees;
                    } else {
                        return Ok(None);
                    }
                } else {
                    // Path component not found
                    return Ok(None);
                }
            }
        }
        
        Ok(None)
    }

    /// Compute the diff between a file in the worktree and its previous version from HEAD,
    /// saving the fragment objects and returning their hashes
    /// 
    /// # Arguments
    /// * `path` - The path to the file in the worktree
    /// 
    /// # Returns
    /// * `Ok(Vec<Hash>)` - The hashes of the saved fragment objects
    ///   - If no previous version exists, returns a single hash for an ADDED fragment with the entire file
    ///   - If a previous version exists, returns hashes for the diff fragments between old and new
    /// * `Err` - If there was an error reading the file or accessing the repository
    pub fn diff_file_with_previous(&self, path: &PathBuf) -> RepoResult<Vec<Hash>> {
        // Read the current file from the worktree
        let mut current_file = File::open(path)?;
        
        // Try to get the previous version from HEAD
        let previous_version = self.get_previous_file_version(path)?;
        
        let diff_fragments = match previous_version {
            None => {
                // No previous version - entire file is new
                let mut body = Vec::new();
                current_file.read_to_end(&mut body)?;
                vec![DiffFragment::ADDED { body }]
            }
            Some(prev_file_struct) => {
                // Previous version exists - reconstruct it and compute diff
                
                // Get all the fragment objects from the previous version
                let prev_fragments: Vec<Fragment> = prev_file_struct
                    .fragments
                    .into_iter()
                    .filter_map(|hash| {
                        let obj = self.get_object(hash).ok()??;
                        if let Object::Fragment(frag) = obj {
                            Some(frag)
                        } else {
                            None
                        }
                    })
                    .collect();
                
                // Extract the DiffFragments from Fragment wrappers
                let prev_diff_fragments: Vec<DiffFragment> = prev_fragments
                    .into_iter()
                    .map(|f| f.0)
                    .collect();
                
                // Reconstruct the previous version in memory
                let mut prev_content = Vec::new();
                let empty_old = std::io::Cursor::new(Vec::<u8>::new());
                diff::apply_diff(empty_old, &prev_diff_fragments, &mut prev_content)?;
                
                // Now compute the diff between previous and current version
                let prev_cursor = std::io::Cursor::new(prev_content);
                diff::diff_streams(prev_cursor, current_file, BLOCK_SIZE)?
            }
        };
        
        // Save each diff fragment as an object and collect the hashes
        let mut hashes = Vec::new();
        for frag in diff_fragments {
            let hash = self.save_obj(Object::Fragment(Fragment(frag)))?;
            hashes.push(hash);
        }
        
        Ok(hashes)
    }

    /// Commit a file in the worktree and return the commit hash
    /// 
    /// # Arguments
    /// * `path` - The path to the file in the worktree
    /// * `message` - The commit message
    /// 
    /// # Returns
    /// * `Ok(Hash)` - The hash of the created commit
    /// * `Err` - If there was an error during the commit process
    /// 
    /// # Process
    /// 1. Computes the content hash of the file
    /// 2. Generates diff fragments and saves them as objects
    /// 3. Creates and saves a FileStruct object
    /// 4. Creates and saves a TreeStruct object containing the file
    /// 5. Creates and saves a CommitStruct with metadata
    /// 6. Updates HEAD to point to the new commit
    pub fn commit_file(&self, path: &PathBuf, message: String) -> RepoResult<Hash> {
        // Step 1: Compute the content hash of the file
        let content_hash = {
            let mut file = File::open(path)?;
            Hash::digest_file_stream(&mut file)?
        };
        
        // Step 2: Get the diff fragment hashes (this also saves the fragments)
        let fragment_hashes = self.diff_file_with_previous(path)?;
        
        // Step 3: Create and save the FileStruct
        let file_struct = FileStruct {
            content_hash,
            fragments: fragment_hashes,
        };
        let file_hash = self.save_obj(Object::File(file_struct))?;
        
        // Step 4: Create the file reference
        let file_name = path
            .file_name()
            .ok_or("Invalid file path")?
            .to_str()
            .ok_or("Invalid file name")?
            .to_string();
        
        let file_ref = FileRefStruct {
            name: file_name,
            mode: 0o644, // Regular file permissions
            hash: file_hash,
        };
        
        // Step 5: Build the tree structure from the path
        // We need to create trees for each directory in the path
        let mut tree_hash = {
            // Start with a tree containing just the file
            let leaf_tree = TreeStruct {
                trees: vec![],
                files: vec![file_ref],
            };
            self.save_obj(Object::Tree(leaf_tree))?
        };
        
        // Work backwards through the path components to build parent trees
        let components: Vec<String> = path
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(String::from))
            .collect();
        
        // Skip the last component (the filename) and work backwards through directories
        if components.len() > 1 {
            for component in components[..components.len() - 1].iter().rev() {
                let tree_ref = TreeRefStruct {
                    name: component.clone(),
                    hash: tree_hash,
                };
                
                let parent_tree = TreeStruct {
                    trees: vec![tree_ref],
                    files: vec![],
                };
                
                tree_hash = self.save_obj(Object::Tree(parent_tree))?;
            }
        }
        
        // Step 6: Get the parent commit (if any)
        let parent_hash = match self.get_head_commit()? {
            Some(head_commit) => {
                // Get the hash of the HEAD commit
                let head_ref = ObjectReference::from_str("HEAD")?;
                self.resolve_ref_name(head_ref)?
            }
            None => {
                // No previous commit - use empty hash
                Hash::new()
            }
        };
        
        // Step 7: Create the root tree reference
        let root_tree_name = if components.is_empty() {
            String::from(".")
        } else {
            components[0].clone()
        };
        
        let root_tree_ref = TreeRefStruct {
            name: root_tree_name,
            hash: tree_hash,
        };
        
        // Step 8: Create and save the commit
        let commit = CommitStruct {
            parent: parent_hash,
            trees: vec![root_tree_ref],
            message,
            comitter: self.me.clone(),
            author: self.me.clone(),
        };
        
        let commit_hash = self.save_obj(Object::Commit(commit))?;
        
        // Step 9: Update HEAD to point to the new commit
        self.set_ref("HEAD", ObjectReference::Hash(commit_hash.clone()))?;
        
        Ok(commit_hash)
    }

}

fn get_path_in_metadata(path: &str) -> PathBuf {
    let mut p = PathBuf::new();
    p.push(REPO_METADATA_DIR_NAME);
    p.push(path);
    return p;
}
