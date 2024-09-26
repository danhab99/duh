use crate::{
    hash::Hash,
    utils::{self, find_file, REPO_CONFIG_FILE_NAME, REPO_METADATA_DIR_NAME},
};
use std::{
    borrow::BorrowMut,
    env,
    error::Error,
    fs::{self, File},
    io::{self, Read, Write},
    path::PathBuf,
    str::FromStr,
};

use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use toml;

pub struct Repo {
    root_path: String,
    buffer_size: usize,
    me: Person,
}

// macro_rules! assert_variant {
//     ($enum:ident, $variant:path) => {
//         match $enum {
//             $variant(x) => x,
//             _ => return Err(NoRepo::new("not the right variant")),
//         }
//     };
// }

impl Repo {
    pub fn at_root_path(root_path: Option<String>) -> Result<Repo, Box<dyn Error>> {
        let rp = match root_path {
            Some(x) => x,
            None => {
                let cwd = env::current_dir()?;
                let c = cwd.to_str().ok_or("cannot identify dir")?;
                String::from(c)
            }
        };

        let config_path = find_file(rp.as_str(), REPO_CONFIG_FILE_NAME)?;

        let content = fs::read(config_path)?;
        let decoded = String::from_utf8(content)?;
        let config = decoded.parse::<toml::Table>()?;

        let user_config = config
            .get("user")
            .ok_or("missing user config")?
            .as_table()
            .ok_or("user config isn't a table")?;

        Ok(Repo {
            root_path: find_file(rp.as_str(), REPO_METADATA_DIR_NAME)?,
            buffer_size: 1000,
            me: Person {
                name: String::from(
                    user_config
                        .get("name")
                        .ok_or("missing user.name")?
                        .as_str()
                        .unwrap_or("missing user.name"),
                ),
                email: String::from(
                    user_config
                        .get("email")
                        .ok_or("missing user.email")?
                        .as_str()
                        .unwrap_or("missing user.email"),
                ),
                timestamp: 0u64,
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

    pub fn initalize_at(root_path: String) -> Result<Repo, Box<dyn Error>> {
        fs::create_dir_all(get_path_in_metadata("objects"))?;
        fs::create_dir_all(get_path_in_metadata("refs"))?;
        fs::write(get_path_in_metadata("config"), "# duh config")?;
        fs::write(get_path_in_metadata("HEAD"), "")?;

        Ok(Repo::at_root_path(Some(root_path))?)
    }

    fn get_object_path(&self, r: ObjectReference) -> Result<PathBuf, Box<dyn Error>> {
        let hash = self.resolve_ref_name(r)?.to_string();
        let top = hex::encode(&hash[0..2]);
        let bottom = hex::encode(&hash[2..hash.len()]);

        Ok(self.get_path_in_repo(format!("objects/{}/{}", top, bottom).as_str()))
    }

    fn save_obj(&self, o: Object) -> Result<Hash, Box<dyn Error>> {
        let (msgpack, hash) = o.hash()?;
        let path = self.get_object_path(ObjectReference::Hash(hash.clone()))?;
        fs::write(path, msgpack)?;
        Ok(hash)
    }

    fn read_object(&self, r: Hash) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        let path = self.get_object_path(ObjectReference::Hash(r))?;
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read(path)?;
        return Ok(Some(content.to_vec()));
    }

    fn get_object(&self, r: Hash) -> Result<Option<Object>, Box<dyn Error>> {
        let content = self.read_object(r)?;
        match content {
            Some(content) => Ok(Some(Object::from_msgpack(content)?)),
            None => Ok(None),
        }
    }

    fn get_ref_path(&self, name: &str) -> PathBuf {
        return self.get_path_in_repo(format!("refs/{}", name).as_str());
    }

    fn set_ref(&self, name: &str, r: ObjectReference) -> Result<(), Box<dyn Error>> {
        let path = self.get_ref_path(name);
        fs::write(
            path,
            match r {
                ObjectReference::Hash(h) => h.to_string(),
                ObjectReference::Ref(r) => format!("ref:{}", r),
            },
        )?;
        return Ok(());
    }

    fn get_ref(&self, name: String) -> Result<ObjectReference, Box<dyn Error>> {
        let ref_path = self.get_path_in_repo(format!("refs/{}", name).as_str());
        let val = String::from_utf8(fs::read(ref_path)?)?;
        if val.starts_with("ref:") {
            Ok(ObjectReference::Ref(val))
        } else {
            let h = Hash::from_string(val);
            Ok(ObjectReference::Hash(h))
        }
    }

    fn resolve_ref_name(&self, refname: ObjectReference) -> Result<Hash, Box<dyn Error>> {
        match refname {
            ObjectReference::Hash(h) => Ok(h),
            ObjectReference::Ref(r) => {
                let n = self.get_ref(r)?;
                return Ok(self.resolve_ref_name(n)?);
            }
        }
    }

    fn is_hash_set(&self, h: Hash) -> Result<bool, Box<dyn Error>> {
        let x = self.get_object_path(ObjectReference::Hash(h))?.exists();
        return Ok(x);
    }

    fn get_index(&self) -> Result<Index, Box<dyn Error>> {
        let bin = fs::read(self.get_path_in_repo("index"))?;
        let mut d = Deserializer::new(bin.as_slice());
        let index = Index::deserialize(&mut d)?;
        Ok(index)
    }

    fn save_index(&self, index: Index) -> Result<(), Box<dyn Error>> {
        let mut buf = Vec::new();
        index.serialize(&mut Serializer::new(&mut buf))?;
        Ok(())
    }

    pub fn stage_file(&self, fp: &str) -> Result<Hash, Box<dyn Error>> {
        let file_path = PathBuf::from_str(fp)?;
        let mut hashes = Vec::<Hash>::new();
        let mut digester = Sha256::new();
        let mut c = self.buffer_size + 1;
        let mut f = File::open(file_path.clone())?;

        while c < self.buffer_size {
            let mut buf = Vec::<u8>::with_capacity(self.buffer_size);
            c = f.read(&mut buf)?;
            digester.write(buf.as_slice())?;
            let frag_hash = self.save_obj(Object::Fragment(Fragment(buf)))?;
            hashes.push(frag_hash);
        }

        let hash = digester.finalize().to_vec();

        let fs = Object::File(FileStruct {
            content_hash: Hash::from_slice(hash.as_slice()),
            fragments: hashes,
        });

        let fs_hash = self.save_obj(fs)?;

        let mut index = self.get_index()?;
        index.staged_files.push(StagedFile {
            content_hash: Hash::from_slice(&hash.as_slice()),
            filestruct_hash: fs_hash,
            file_path: file_path.clone(),
        });
        self.save_index(index)?;

        Ok(Hash::from_slice(hash.as_slice()))
    }

    fn build_tree(&self, start: &str) -> Result<TreeStruct, Box<dyn Error>> {
        let mut tree = TreeStruct {
            trees: Vec::new(),
            files: Vec::new(),
        };

        let dir = self.get_path_in_cwd(start);
        let index = self.get_index()?;

        for d in fs::read_dir(dir)? {
            let entry = d?;
            if entry.metadata()?.is_dir() {
                let t = self.build_tree(entry.path().as_os_str().to_str().unwrap())?;
                let (_, h) = Object::Tree(t).hash()?;
                let path = entry.path();
                let ll = path.iter().last().unwrap();
                let l = ll.to_str().unwrap();

                tree.borrow_mut().trees.push(TreeRefStruct {
                    name: l.to_string(),
                    hash: h,
                });
            } else {
                let res = index
                    .staged_files
                    .iter()
                    .find(|x| x.file_path.starts_with(entry.path()));

                let fs_hash: Hash = match res {
                    None => {
                        let mut digester = Sha256::new();
                        let mut file = File::open(entry.path())?;
                        io::copy(&mut file, &mut digester)?;
                        let content_hash = digester.finalize().to_vec();

                        let fragments = Vec::<Hash>::new();

                        let fs = FileStruct {
                            content_hash: Hash::from_slice(content_hash.as_slice()),
                            fragments,
                        };

                        self.save_obj(Object::File(fs.clone()));

                        fs.content_hash
                    }
                    Some(info) => info.filestruct_hash.clone(),
                };

                tree.borrow_mut().files.push(FileRefStruct {
                    hash: fs_hash,
                    name: entry.file_name().into_string().unwrap(),
                    mode: 0u16,
                });
            }
        }

        self.save_obj(Object::Tree(tree.clone()))?;

        return Ok(tree);
    }

    pub fn commit(&self, message: String, start: &str) -> Result<Hash, Box<dyn Error>> {
        let tree = self.build_tree(start)?;
        let (_, tree_hash) = Object::Tree(tree).hash()?;

        let commit = Object::Commit(CommitStruct {
            parent: Hash::new(),
            tree: TreeRefStruct {
                name: String::from_str("")?,
                hash: tree_hash,
            },
            message,
            comitter: self.me.clone(),
            author: self.me.clone(),
        });

        return self.save_obj(commit);
    }
}

pub enum ObjectReference {
    Hash(Hash),
    Ref(String),
}

impl ObjectReference {
    pub fn ref_from_str(s: &str) -> Result<ObjectReference, Box<dyn Error>> {
        Ok(ObjectReference::Ref(String::from_str(s)?)) // TODO ref:
    }
}

fn hash_string(txt: String) -> Result<String, Box<dyn Error>> {
    let x = String::from_utf8(Sha256::digest(txt.clone()).to_vec())?;
    Ok(x)
}

fn get_path_in_metadata(path: &str) -> PathBuf {
    let mut p = PathBuf::new();
    p.push(REPO_METADATA_DIR_NAME);
    p.push(path);
    return p;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct StagedFile {
    content_hash: Hash,
    filestruct_hash: Hash,
    file_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Index {
    staged_files: Vec<StagedFile>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Fragment(Vec<u8>);
#[derive(Debug, Serialize, Deserialize, Clone)]
struct TreeRefStruct {
    name: String,
    hash: Hash,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
struct FileRefStruct {
    name: String,
    mode: u16,
    hash: Hash,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Person {
    name: String,
    email: String,
    timestamp: u64,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommitStruct {
    parent: Hash,
    tree: TreeRefStruct,
    message: String,
    comitter: Person,
    author: Person,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TreeStruct {
    trees: Vec<TreeRefStruct>,
    files: Vec<FileRefStruct>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileStruct {
    content_hash: Hash,
    fragments: Vec<Hash>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Object {
    Commit(CommitStruct),
    Tree(TreeStruct),
    File(FileStruct),
    Fragment(Fragment),
}

impl Object {
    fn to_msgpack(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.serialize(&mut Serializer::new(&mut buf)).unwrap();
        buf.to_vec()
    }

    fn from_msgpack(bin: Vec<u8>) -> Result<Object, Box<dyn Error>> {
        let mut d = Deserializer::new(bin.as_slice());
        let o = Object::deserialize(&mut d)?;
        Ok(o)
    }

    fn hash(&self) -> Result<(Vec<u8>, Hash), Box<dyn Error>> {
        let msgpack = self.to_msgpack();
        // let hash = Hash::from_string(String::from_utf8(Sha256::digest(msgpack.clone()).to_vec())?);
        let hash = Hash::from_string(hash_string(String::from_utf8(self.to_msgpack())?)?);

        return Ok((msgpack, hash));
    }
}
