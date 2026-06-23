use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs::OpenOptions,
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use crate::error::NoSpace;

static LOG_WRITER: OnceLock<Mutex<Option<BufWriter<std::fs::File>>>> = OnceLock::new();

/// Initialize the diagnostic log file. Returns Ok(()) if successfully opened.
/// Subsequent calls are no-ops (first writer wins).
pub fn init_log(path: &str) -> std::io::Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let writer = BufWriter::new(file);
    let _ = LOG_WRITER.set(Mutex::new(Some(writer)));
    Ok(())
}

/// Write a line to the diagnostic log file (if initialized).
pub fn log_to_file(line: &str) {
    if let Some(writer) = LOG_WRITER.get() {
        if let Ok(mut guard) = writer.lock() {
            if let Some(ref mut w) = *guard {
                let _ = writeln!(w, "{}", line);
                let _ = w.flush();
            }
        }
    }
}

/// Check if diagnostic logging is active.
pub fn log_active() -> bool {
    if let Some(writer) = LOG_WRITER.get() {
        if let Ok(guard) = writer.lock() {
            return guard.is_some();
        }
    }
    false
}

pub fn get_cwd() -> String {
    std::env::current_dir()
        .unwrap()
        .to_str()
        .unwrap_or_default()
        .to_owned()
}

/// Returns true when `DUH_VERBOSE` env var is set to `1`, `true` or `yes`.
pub fn verbose_enabled() -> bool {
    std::env::var("DUH_VERBOSE")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes"
        })
        .unwrap_or(false)
}

/// Verbose logging macro. Prints only when `DUH_VERBOSE` is enabled.
/// Usage: `vlog!("details: {}", val);`
#[macro_export]
macro_rules! vlog {
    ($($arg:tt)*) => {
        if $crate::utils::verbose_enabled() {
            println!($($arg)*);
        }
    };
}

pub const SPACE_METADATA_DIR_NAME: &str = ".duh";
pub fn get_space_config_file_name() -> String {
    return format!("{}/{}", SPACE_METADATA_DIR_NAME, "config");
}
pub fn get_space_ignore_file_name() -> String {
    return format!("{}/{}", SPACE_METADATA_DIR_NAME, "ignore");
}

/// Walk up from `start_path` looking for a `.duh` directory.
///
/// Returns `(metadata_dir, worktree_dir)`:
/// - If `.duh` is a directory containing a `config` file, it's a normal repo:
///   - `metadata_dir` = path to `.duh`
///   - `worktree_dir` = parent directory (where `.duh` lives)
/// - If `.duh` is a file, it's a bare repo pointing to a metadata directory:
///   - `metadata_dir` = path read from the file
///   - `worktree_dir` = `None` (bare repo, no working tree)
///
/// Returns `Err(NoSpace)` if no `.duh` is found before reaching `/`.
pub fn find_duh_dir(start_path: &str) -> Result<(PathBuf, Option<PathBuf>), Box<dyn Error>> {
    let mut path = PathBuf::from(start_path);

    loop {
        let mut p = path.clone();

        if p.eq(&PathBuf::from("/")) {
            return Err(Box::new(NoSpace {
                details: String::from("not inside a duh spacesitory"),
            }));
        }

        p.push(SPACE_METADATA_DIR_NAME);
        vlog!("Checking path {}", p.display());

        if p.exists() {
            // Found .duh — determine if it's a file (bare) or directory (normal)
            if p.is_file() {
                // Bare repo: .duh is a file containing the path to the metadata directory
                let metadata_path_str = std::fs::read_to_string(&p)?;
                let metadata_path = PathBuf::from(metadata_path_str.trim());
                return Ok((metadata_path, None));
            } else {
                // Normal repo: .duh is a directory
                let worktree = path.clone();
                return Ok((p, Some(worktree)));
            }
        }

        if !path.pop() {
            break;
        }
    }

    Err(Box::new(NoSpace {
        details: String::from("not inside a duh spacesitory"),
    }))
}

pub fn find_file(start_path: &str, target: &str) -> Result<String, Box<dyn Error>> {
    let mut path = PathBuf::from(start_path);

    loop {
        let mut p = path.clone();

        if p.eq(&PathBuf::from("/")) {
            return Err(Box::new(NoSpace {
                details: String::from("not inside a duh spacesitory"),
            }));
        }

        p.push(target);
        vlog!("Checking path {}", p.display());

        if Path::new(p.to_str().unwrap_or("")).exists() {
            break;
        }

        if !path.pop() {
            break;
        }
    }

    Ok(format!(
        "{}/{}",
        String::from(path.to_str().unwrap()),
        target
    ))
}

pub fn hash_string(txt: String) -> Result<String, Box<dyn Error>> {
    let digest = Sha256::digest(txt.as_bytes());
    Ok(hex::encode(digest))
}

pub fn hash_bytes(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    hex::encode(digest)
}

pub fn read_chunk<R: Read>(reader: &mut R, size: usize) -> std::io::Result<(Vec<u8>, bool)> {
    if size == 0 {
        return Ok((vec![0u8; 0], false));
    }
    let mut buf = vec![0u8; size];
    let n = reader.read(&mut buf)?;

    if n == 0 {
        Ok((Vec::new(), true))
    } else {
        buf.truncate(n);
        Ok((buf, false))
    }
}
