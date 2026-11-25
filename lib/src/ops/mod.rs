//! Every operation duh can do, as plain functions that return data and never
//! print. The CLI and duh Studio are both thin shells over this module, so
//! they behave the same because they run the same code.
//!
//! Conventions:
//! - Paths are repo paths (relative to the space's root, `/`-separated).
//!   Turning "relative to where you are" or a glob into repo paths is the
//!   caller's business.
//! - Operations that change the picked set save it before returning.
//! - Errors are sentences fit to show a person as they are.

use std::error::Error;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::diff::DiffFragment;
use crate::objects::FileFragment;
use crate::space::Space;

pub mod compare;
pub mod files;
pub mod history;
pub mod look;
pub mod pick;
pub mod refs;
pub mod save;
pub mod sharing;
pub mod worktree;

pub type OpResult<T> = Result<T, Box<dyn Error>>;

/// One run of bytes in a change strip: how a file differs from the version
/// before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span {
    /// Bytes carried over unchanged.
    Kept(u64),
    /// New bytes.
    Added(u64),
    /// Bytes that were there before and are gone.
    Removed(u64),
}

impl Span {
    pub fn len(&self) -> u64 {
        match *self {
            Span::Kept(n) | Span::Added(n) | Span::Removed(n) => n,
        }
    }
}

impl From<&DiffFragment> for Span {
    fn from(f: &DiffFragment) -> Self {
        match f {
            DiffFragment::UNCHANGED { len } => Span::Kept(*len as u64),
            DiffFragment::DELETED { len } => Span::Removed(*len as u64),
            DiffFragment::ADDED { body } => Span::Added(body.len() as u64),
        }
    }
}

impl From<&FileFragment> for Span {
    fn from(f: &FileFragment) -> Self {
        match f {
            FileFragment::UNCHANGED { len } => Span::Kept(*len as u64),
            FileFragment::DELETED { len } => Span::Removed(*len as u64),
            FileFragment::ADDED { len, .. } => Span::Added(*len as u64),
        }
    }
}

/// Bytes added, removed and kept across a strip.
pub fn strip_totals(strip: &[Span]) -> (u64, u64, u64) {
    let mut t = (0, 0, 0);
    for s in strip {
        match *s {
            Span::Added(n) => t.0 += n,
            Span::Removed(n) => t.1 += n,
            Span::Kept(n) => t.2 += n,
        }
    }
    t
}

/// "1.2 MB", "340 B": a byte count for people.
pub fn fmt_bytes(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1} GB", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.1} MB", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1} KB", n as f64 / 1_000.0)
    } else {
        format!("{} B", n)
    }
}

/// Open the space containing `path` (any folder inside it, or its root).
pub fn open(path: &Path) -> OpResult<Space> {
    let start = path.to_str().ok_or("that folder's name isn't valid text")?;
    let (metadata_dir, worktree) = crate::utils::find_duh_dir(start)?;
    Space::open(crate::store::local(&metadata_dir)?, worktree)
}

/// Held while changing a space; released when dropped. Readers don't take
/// it, so looking at a space never waits.
pub struct WriteLock(#[allow(dead_code)] File);

/// Wait (up to `patience`) for the right to change the space containing
/// `path`, so the CLI and duh Studio never write to one space at once.
pub fn lock(path: &Path, patience: Duration) -> OpResult<WriteLock> {
    let start = path.to_str().ok_or("that folder's name isn't valid text")?;
    let (metadata_dir, _) = crate::utils::find_duh_dir(start)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(metadata_dir.join("lock"))?;
    let deadline = Instant::now() + patience;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(WriteLock(file)),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err("another duh is changing this space right now; try again in a moment".into())
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use tempfile::TempDir;

    /// A new space with working files, in a temporary folder.
    pub fn space() -> (TempDir, Space) {
        let dir = TempDir::new().unwrap();
        let meta = dir.path().join(crate::utils::SPACE_METADATA_DIR_NAME);
        std::fs::create_dir_all(&meta).unwrap();
        let space = Space::init(crate::store::local(&meta).unwrap(), Some(dir.path().into())).unwrap();
        (dir, space)
    }

    pub fn write(dir: &TempDir, path: &str, data: &[u8]) {
        let p = dir.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }

    pub fn noise(seed: u64, len: usize) -> Vec<u8> {
        let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_writer_waits_then_gives_up() {
        let (dir, _space) = testing::space();
        let held = lock(dir.path(), Duration::from_secs(1)).unwrap();
        let err = lock(dir.path(), Duration::from_millis(200)).err().unwrap();
        assert!(err.to_string().contains("another duh"));
        drop(held);
        assert!(lock(dir.path(), Duration::from_millis(200)).is_ok());
    }

    #[test]
    fn open_finds_the_space_from_a_subfolder() {
        let (dir, _space) = testing::space();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        let space = open(&dir.path().join("a/b")).unwrap();
        assert_eq!(space.worktree.as_deref(), Some(dir.path()));
    }
}
