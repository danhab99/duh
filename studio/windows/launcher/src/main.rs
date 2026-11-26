//! duh Studio for Windows as a single file.
//!
//! Flutter's Windows apps are a folder: the program, the engine DLL, duh's
//! engine DLL and a `data/` folder. This launcher carries that folder inside
//! itself. On first start it unpacks it to
//! `%LOCALAPPDATA%\duh-studio\<version>\`, then runs the app from there, passing
//! along every argument and the exit code. Later starts skip the unpacking.
//!
//! The payload is a plain list of files built by Nix: for each file, its path
//! length (u32, little endian), its path (UTF-8, `/`-separated), its size
//! (u64), then its bytes.

#![windows_subsystem = "windows"]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

static PAYLOAD: &[u8] = include_bytes!(env!("DUH_STUDIO_PAYLOAD"));
const PAYLOAD_ID: &str = env!("DUH_STUDIO_PAYLOAD_ID");
const APP: &str = "duh_studio.exe";

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("duh Studio could not start: {e}");
            1
        }
    };
    exit(code);
}

fn run() -> io::Result<i32> {
    let home = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("duh-studio");
    let dir = home.join(PAYLOAD_ID);
    if !dir.join(APP).is_file() {
        unpack(&home, &dir)?;
        forget_old_versions(&home);
    }
    let status = Command::new(dir.join(APP))
        .args(std::env::args_os().skip(1))
        .status()?;
    Ok(status.code().unwrap_or(1))
}

/// Unpack beside the final folder, then move it into place, so a start that
/// is interrupted never leaves a half-written app behind.
fn unpack(home: &Path, dir: &Path) -> io::Result<()> {
    let staging = home.join(format!("{PAYLOAD_ID}.{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    let mut rest = PAYLOAD;
    while !rest.is_empty() {
        let name_len = u32::from_le_bytes(take(&mut rest, 4)?.try_into().unwrap()) as usize;
        let name = std::str::from_utf8(take(&mut rest, name_len)?)
            .map_err(|_| bad("a file name is not UTF-8"))?;
        let size = u64::from_le_bytes(take(&mut rest, 8)?.try_into().unwrap()) as usize;
        let bytes = take(&mut rest, size)?;
        if name.split('/').any(|part| part.is_empty() || part == "..") {
            return Err(bad("a file name is not a plain relative path"));
        }
        let path = staging.join(name);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, bytes)?;
    }
    match fs::rename(&staging, dir) {
        Ok(()) => Ok(()),
        // Another start of the app unpacked it first.
        Err(_) if dir.join(APP).is_file() => {
            let _ = fs::remove_dir_all(&staging);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// After an update, the versions unpacked before it are no longer needed.
/// A version still running can't be deleted on Windows, so it is skipped and
/// cleared next time.
fn forget_old_versions(home: &Path) {
    let Ok(entries) = fs::read_dir(home) else { return };
    for entry in entries.flatten() {
        if entry.file_name() != PAYLOAD_ID {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn take<'a>(rest: &mut &'a [u8], n: usize) -> io::Result<&'a [u8]> {
    if rest.len() < n {
        return Err(bad("the app inside this file is cut short"));
    }
    let (head, tail) = rest.split_at(n);
    *rest = tail;
    Ok(head)
}

fn bad(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}
