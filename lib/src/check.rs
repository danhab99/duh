//! Checking that a space is intact.
//!
//! Three layers, each catching what the one before can't:
//!
//! 1. **Stored objects**: every object's bytes still hash to its name and
//!    decode. Catches bit rot and truncated writes.
//! 2. **Links**: every save reachable from a draft, label or journal entry
//!    has its tree, file versions and fragments present, of the right kind
//!    and length. Catches objects that went missing.
//! 3. **Rebuilds**: every file version of those saves, everything picked
//!    for the next save, and every solid copy is rebuilt from its pieces and
//!    hashed. Catches anything that leaves objects intact but reads back
//!    wrong.

use std::collections::HashSet;
use std::io::Read;

use sha2::{Digest, Sha256};

use crate::file::FileOps;
use crate::hash::Hash;
use crate::objects::{FileFragment, Object};
use crate::space::{Space, SpaceResult};

/// What a check looked at, and everything it found wrong.
#[derive(Debug, Default)]
pub struct CheckReport {
    /// Stored objects whose bytes were hashed.
    pub objects: usize,
    /// Saves reachable from drafts, labels and the journal.
    pub saves: usize,
    /// Distinct file versions rebuilt and hashed.
    pub file_versions: usize,
    /// Files picked for the next save, rebuilt and hashed.
    pub picked: usize,
    /// Solid copies rebuilt and hashed.
    pub solids: usize,
    /// One plain sentence per problem found.
    pub problems: Vec<String>,
}

impl CheckReport {
    pub fn is_intact(&self) -> bool {
        self.problems.is_empty()
    }
}

/// The steps of a check, reported as they start.
pub enum CheckStep {
    Objects,
    Saves,
    Picked,
    Solids,
}

fn short(h: &Hash) -> String {
    h.to_hex()[..12].to_string()
}

/// SHA-256 of everything `r` yields.
fn digest_reader(mut r: impl Read) -> std::io::Result<Hash> {
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Hash::from_string(hex::encode(h.finalize()))
        .map_err(|e| std::io::Error::other(e.to_string()))
}

impl Space {
    /// Check that everything in this space is intact. Problems go in the
    /// report; an `Err` means the check itself couldn't run.
    pub fn check<P: FnMut(CheckStep)>(&mut self, mut step: P) -> SpaceResult<CheckReport> {
        let mut report = CheckReport::default();

        step(CheckStep::Objects);
        let damaged = self.check_objects(&mut report)?;

        step(CheckStep::Saves);
        let roots = self.check_roots(&mut report)?;
        self.check_saves(roots, &damaged, &mut report)?;

        step(CheckStep::Picked);
        self.check_picked(&mut report)?;

        step(CheckStep::Solids);
        self.check_solids(&mut report)?;

        Ok(report)
    }

    /// Layer 1: hash and decode every stored object. Returns the damaged
    /// ones, so later layers don't report them twice.
    fn check_objects(&self, report: &mut CheckReport) -> SpaceResult<HashSet<Hash>> {
        let mut damaged = HashSet::new();
        if !self.store().exists("objects/")? {
            return Ok(damaged);
        }
        for entry in self.store().list_with("objects/").recursive(true).call()? {
            if !entry.metadata().is_file() {
                continue;
            }
            let key = entry.path();
            let name: String = key.trim_start_matches("objects/").chars().filter(|c| *c != '/').collect();
            let Ok(hash) = Hash::from_string(name) else {
                report.problems.push(format!("'{}' in the object store isn't named like an object", key));
                continue;
            };
            report.objects += 1;
            let bytes = self.store().read(key)?.to_vec();
            if Hash::digest_slice(&bytes)? != hash {
                report.problems.push(format!("stored object {} is damaged: its bytes have changed", short(&hash)));
                damaged.insert(hash);
            } else if Object::from_msgpack(bytes).is_err() {
                report.problems.push(format!("stored object {} is damaged: it can't be read", short(&hash)));
                damaged.insert(hash);
            }
        }
        Ok(damaged)
    }

    /// Every save a draft, label or journal entry points at, with where
    /// that pointer lives (for messages).
    fn check_roots(&self, report: &mut CheckReport) -> SpaceResult<Vec<(Hash, String)>> {
        let mut roots = Vec::new();
        for dir in ["refs/", "reflog/"] {
            if !self.store().exists(dir)? {
                continue;
            }
            for entry in self.store().list_with(dir).recursive(true).call()? {
                if !entry.metadata().is_file() {
                    continue;
                }
                let key = entry.path().to_string();
                let text = String::from_utf8_lossy(&self.store().read(&key)?.to_vec()).to_string();
                for line in text.lines() {
                    // Refs hold a hash (or, for HEAD, a draft's name); journal
                    // lines are "<hash> | <message>".
                    let word = line.split('|').next().unwrap_or("").trim();
                    let is_hash = word.len() == 64 && word.chars().all(|c| c.is_ascii_hexdigit());
                    if !is_hash {
                        continue;
                    }
                    match Hash::from_string(word.to_string()) {
                        Ok(h) if !h.is_zero() => roots.push((h, key.clone())),
                        Ok(_) => {}
                        Err(_) => report.problems.push(format!("'{}' holds a hash that can't be read", key)),
                    }
                }
            }
        }
        Ok(roots)
    }

    /// Layers 2 and 3 for every save reachable from `roots`.
    fn check_saves(
        &mut self,
        roots: Vec<(Hash, String)>,
        damaged: &HashSet<Hash>,
        report: &mut CheckReport,
    ) -> SpaceResult<()> {
        let mut seen_saves = HashSet::new();
        let mut seen_versions = HashSet::new();
        for (root, from) in roots {
            let mut cur = root;
            while !cur.is_zero() && seen_saves.insert(cur) {
                let commit = match self.get_object(cur) {
                    Ok(Some(Object::Commit(c))) => c,
                    Ok(None) => {
                        report.problems.push(format!("save {} (reached from {}) is missing", short(&cur), from));
                        break;
                    }
                    _ if damaged.contains(&cur) => break,
                    _ => {
                        report.problems.push(format!("{} (reached from {}) isn't a save", short(&cur), from));
                        break;
                    }
                };
                report.saves += 1;
                let files = match self.get_commit_files(cur) {
                    Ok(f) => f,
                    Err(e) => {
                        report.problems.push(format!("save {}: its file list can't be read: {}", short(&cur), e));
                        cur = commit.parent;
                        continue;
                    }
                };
                let mut paths: Vec<_> = files.into_iter().collect();
                paths.sort_by(|a, b| a.0.cmp(&b.0));
                for (path, version) in paths {
                    if !seen_versions.insert(version) {
                        continue;
                    }
                    report.file_versions += 1;
                    self.check_version(&path, version, cur, damaged, report)?;
                }
                cur = commit.parent;
            }
        }
        Ok(())
    }

    /// One file version: its pieces are present and right, and it rebuilds
    /// to the bytes it was saved with.
    fn check_version(
        &mut self,
        path: &str,
        version: Hash,
        commit: Hash,
        damaged: &HashSet<Hash>,
        report: &mut CheckReport,
    ) -> SpaceResult<()> {
        let at = format!("{} in save {}", path, short(&commit));
        let file = match self.get_object(version) {
            Ok(Some(Object::File(f))) => f,
            Ok(None) => {
                report.problems.push(format!("{}: its version {} is missing", at, short(&version)));
                return Ok(());
            }
            _ if damaged.contains(&version) => return Ok(()),
            _ => {
                report.problems.push(format!("{}: {} isn't a file version", at, short(&version)));
                return Ok(());
            }
        };

        let mut pieces_ok = true;
        for frag in &file.fragments {
            match self.get_object(*frag) {
                Ok(Some(Object::FileDiffFragment(FileFragment::ADDED { body, len }))) => {
                    match self.get_object(body) {
                        Ok(Some(Object::Fragment(f))) if f.0.len() == len => {}
                        Ok(Some(Object::Fragment(f))) => {
                            report.problems.push(format!(
                                "{}: piece {} holds {} bytes, not {}",
                                at, short(&body), f.0.len(), len
                            ));
                            pieces_ok = false;
                        }
                        Ok(None) => {
                            report.problems.push(format!("{}: piece {} is missing", at, short(&body)));
                            pieces_ok = false;
                        }
                        _ if damaged.contains(&body) => pieces_ok = false,
                        _ => {
                            report.problems.push(format!("{}: {} isn't a piece of a file", at, short(&body)));
                            pieces_ok = false;
                        }
                    }
                }
                Ok(Some(Object::FileDiffFragment(_))) => {}
                Ok(None) => {
                    report.problems.push(format!("{}: step {} of its changes is missing", at, short(frag)));
                    pieces_ok = false;
                }
                _ if damaged.contains(frag) => pieces_ok = false,
                _ => {
                    report.problems.push(format!("{}: {} isn't a step of its changes", at, short(frag)));
                    pieces_ok = false;
                }
            }
        }

        // A version can rebuild through a solid copy, or through earlier
        // versions, even when its own pieces are damaged; check either way.
        let rebuilt = FileOps::from_space(self)
            .open_file(path.to_string(), commit)
            .map_err(|e| e.to_string())
            .and_then(|r| digest_reader(r).map_err(|e| e.to_string()));
        match rebuilt {
            Ok(h) if h == file.content_hash => {}
            Ok(_) => report.problems.push(format!("{}: rebuilds to different bytes than were saved", at)),
            // Missing or damaged pieces were already reported above.
            Err(_) if !pieces_ok => {}
            Err(e) => report.problems.push(format!("{}: can't be rebuilt: {}", at, e)),
        }
        Ok(())
    }

    /// Layer 3 for files picked for the next save.
    fn check_picked(&mut self, report: &mut CheckReport) -> SpaceResult<()> {
        let mut picked: Vec<(String, Hash)> = self
            .index
            .iter()
            .filter(|(_, v)| !v.is_zero())
            .map(|(p, v)| (p.clone(), *v))
            .collect();
        picked.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, version) in picked {
            report.picked += 1;
            let want = match self.get_object(version) {
                Ok(Some(Object::File(f))) => f.content_hash,
                _ => {
                    report.problems.push(format!("{} (picked): its version {} is missing or damaged", path, short(&version)));
                    continue;
                }
            };
            let rebuilt = FileOps::from_space(self)
                .open_staged(path.clone())
                .map_err(|e| e.to_string())
                .and_then(|r| digest_reader(r).map_err(|e| e.to_string()));
            match rebuilt {
                Ok(h) if h == want => {}
                Ok(_) => report.problems.push(format!("{} (picked): rebuilds to different bytes than were picked", path)),
                Err(e) => report.problems.push(format!("{} (picked): can't be rebuilt: {}", path, e)),
            }
        }
        Ok(())
    }

    /// Layer 3 for solid copies: each reads back to the content it's for.
    fn check_solids(&self, report: &mut CheckReport) -> SpaceResult<()> {
        for (content, solid) in self.list_solids()? {
            report.solids += 1;
            let at = format!("the solid copy of {}", short(&content));
            let parts = match self.get_object(solid) {
                Ok(Some(Object::Solid(s))) if s.content_hash == content => s.parts,
                Ok(Some(Object::Solid(_))) => {
                    report.problems.push(format!("{} is filed under the wrong content", at));
                    continue;
                }
                _ => {
                    report.problems.push(format!("{} ({}) is missing or damaged", at, short(&solid)));
                    continue;
                }
            };
            let mut h = Sha256::new();
            let mut whole = true;
            for (part, len) in parts {
                match self.get_object(part) {
                    Ok(Some(Object::Fragment(f))) if f.0.len() == len => h.update(&f.0),
                    _ => {
                        report.problems.push(format!("{}: piece {} is missing or damaged", at, short(&part)));
                        whole = false;
                        break;
                    }
                }
            }
            if whole && Hash::from_string(hex::encode(h.finalize()))? != content {
                report.problems.push(format!("{} reads back as different bytes", at));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::DiffFragment;
    use tempfile::TempDir;

    fn noise(seed: u64, len: usize) -> Vec<u8> {
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

    /// A space with a few saves of one file, a solid copy and a pick.
    fn sample() -> (TempDir, Space) {
        let dir = TempDir::new().unwrap();
        let meta = dir.path().join(".duh");
        std::fs::create_dir_all(&meta).unwrap();
        let mut space = Space::init(crate::store::local(&meta).unwrap(), Some(dir.path().into())).unwrap();
        space.solid_after = 2;
        let mut data = noise(1, 100_000);
        for i in 0..4u8 {
            data[i as usize * 10_000] ^= 0xff;
            std::fs::write(dir.path().join("f.bin"), &data).unwrap();
            let mut ops = FileOps::from_space(&mut space);
            ops.stage_file("f.bin".into(), None::<fn(DiffFragment)>, None::<fn(_)>).unwrap();
            ops.commit(format!("save {}", i)).unwrap();
        }
        data.extend_from_slice(b"picked");
        std::fs::write(dir.path().join("f.bin"), &data).unwrap();
        FileOps::from_space(&mut space)
            .stage_file("f.bin".into(), None::<fn(DiffFragment)>, None::<fn(_)>)
            .unwrap();
        (dir, space)
    }

    fn object_files(dir: &TempDir) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![dir.path().join(".duh/objects")];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() { stack.push(p) } else { out.push(p) }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn an_intact_space_checks_clean() {
        let (_dir, mut space) = sample();
        let report = space.check(|_| {}).unwrap();
        assert!(report.is_intact(), "{:?}", report.problems);
        assert_eq!(report.saves, 4);
        assert_eq!(report.file_versions, 4);
        assert_eq!(report.picked, 1);
        assert!(report.solids >= 1);
        assert!(report.objects > 10);
    }

    #[test]
    fn every_damaged_or_missing_object_is_found() {
        // Damage each object in turn, in a fresh space each time: whatever
        // it is (save, tree, version, step, piece, solid copy), the check
        // must notice.
        let count = object_files(&sample().0).len();
        for i in 0..count {
            let (dir, mut space) = sample();
            let target = &object_files(&dir)[i];
            let mut bytes = std::fs::read(target).unwrap();
            let last = bytes.len() - 1;
            bytes[last] ^= 1;
            std::fs::write(target, bytes).unwrap();
            let report = space.check(|_| {}).unwrap();
            assert!(!report.is_intact(), "damaging {} went unnoticed", target.display());

            let (dir, mut space) = sample();
            let target = &object_files(&dir)[i];
            std::fs::remove_file(target).unwrap();
            let report = space.check(|_| {}).unwrap();
            assert!(!report.is_intact(), "deleting {} went unnoticed", target.display());
        }
    }
}
