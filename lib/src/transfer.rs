//! Moving objects between spaces.
//!
//! Spaces mediate every transfer: the source reads an object's stored bytes
//! from its store and the destination writes them to its own, verifying the
//! hash. The two stores can be any opendal backends — copying from a local
//! space to one on S3 is the same call as copying between two directories.

use crate::{
    error::DuhError,
    hash::Hash,
    objects::{FileFragment, Object},
    space::{Space, SpaceResult},
};

/// What a transfer moved.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TransferStats {
    /// Commits newly written to the destination.
    pub commits: usize,
    /// Objects of any kind newly written to the destination.
    pub objects: usize,
    /// Stored bytes newly written to the destination.
    pub bytes: u64,
}

impl Space {
    /// Copy one object's stored bytes from this space to `dest`. Returns the
    /// number of bytes written (0 if `dest` already had it).
    pub fn copy_object_to(&self, dest: &Space, hash: Hash) -> SpaceResult<u64> {
        if dest.has_object(hash)? {
            return Ok(0);
        }
        let bytes = self
            .read_raw_object(hash)?
            .ok_or_else(|| DuhError::object_not_found(&hash.to_string(), "object"))?;
        let len = bytes.len() as u64;
        dest.write_raw_object(hash, bytes)?;
        Ok(len)
    }

    /// Copy `commit` and everything it depends on — ancestors, trees, file
    /// versions and fragment data — that `dest` doesn't already have.
    ///
    /// Commits go oldest first, and every object's dependencies are written
    /// before the object itself, so `dest` never holds an object whose
    /// references are missing, even if the transfer is interrupted. That
    /// invariant is also why finding a commit (or tree) already in `dest`
    /// means everything beneath it is there too.
    ///
    /// Solid copies `dest` doesn't have come along too, including ones made
    /// after their saves were first copied over.
    pub fn copy_commit_to(&self, dest: &Space, commit: Hash) -> SpaceResult<TransferStats> {
        let mut stats = TransferStats::default();
        if commit.is_zero() {
            return Ok(stats);
        }

        // Walk back to the newest ancestor dest already has.
        let mut missing = Vec::new();
        let mut cur = commit;
        while !cur.is_zero() && !dest.has_object(cur)? {
            match self.get_object(cur)? {
                Some(Object::Commit(c)) => {
                    missing.push((cur, c.tree));
                    cur = c.parent;
                }
                _ => return Err(DuhError::object_not_found(&cur.to_string(), "commit").into()),
            }
        }

        for (hash, tree) in missing.into_iter().rev() {
            self.copy_tree_to(dest, tree, &mut stats)?;
            self.count(self.copy_object_to(dest, hash)?, &mut stats);
            stats.commits += 1;
        }
        self.copy_solids_to(dest, &mut stats)?;
        Ok(stats)
    }

    /// Copy every solid copy `dest` lacks: its fragments, then the Solid
    /// object, then the entry that points `dest` at it.
    fn copy_solids_to(&self, dest: &Space, stats: &mut TransferStats) -> SpaceResult<()> {
        for (content, solid) in self.list_solids()? {
            if dest.solid_for(&content)?.is_some() {
                continue;
            }
            let parts = match self.get_object(solid)? {
                Some(Object::Solid(s)) => s.parts,
                _ => return Err(DuhError::object_not_found(&solid.to_string(), "solid copy").into()),
            };
            for (fragment, _) in parts {
                self.count(self.copy_object_to(dest, fragment)?, stats);
            }
            self.count(self.copy_object_to(dest, solid)?, stats);
            dest.set_solid(&content, solid)?;
        }
        Ok(())
    }

    /// Whether `ancestor` is `commit` itself or one of its ancestors, judged
    /// from this space's history (a commit it doesn't have isn't an
    /// ancestor). The zero hash ("no commit") is an ancestor of everything.
    pub fn is_ancestor(&self, ancestor: Hash, commit: Hash) -> SpaceResult<bool> {
        if ancestor.is_zero() {
            return Ok(true);
        }
        let mut cur = commit;
        while !cur.is_zero() {
            if cur == ancestor {
                return Ok(true);
            }
            cur = match self.get_object(cur)? {
                Some(Object::Commit(c)) => c.parent,
                _ => return Ok(false),
            };
        }
        Ok(false)
    }

    /// How many saves lead from `from` to `to` (from `to` back to the first
    /// save, when `from` isn't one of its ancestors or is zero).
    pub fn saves_between(&self, from: Hash, to: Hash) -> SpaceResult<usize> {
        let mut n = 0;
        let mut cur = to;
        while !cur.is_zero() && cur != from {
            n += 1;
            cur = match self.get_object(cur)? {
                Some(Object::Commit(c)) => c.parent,
                _ => break,
            };
        }
        Ok(n)
    }

    fn count(&self, bytes: u64, stats: &mut TransferStats) {
        if bytes > 0 {
            stats.objects += 1;
            stats.bytes += bytes;
        }
    }

    fn copy_tree_to(&self, dest: &Space, tree: Hash, stats: &mut TransferStats) -> SpaceResult<()> {
        if dest.has_object(tree)? {
            return Ok(());
        }
        let entries = match self.get_object(tree)? {
            Some(Object::Tree(t)) => t.entries,
            _ => return Err(DuhError::object_not_found(&tree.to_string(), "tree").into()),
        };
        for entry in entries {
            if entry.mode == 0o40000 {
                self.copy_tree_to(dest, entry.hash, stats)?;
            } else {
                self.copy_file_version_to(dest, entry.hash, stats)?;
            }
        }
        self.count(self.copy_object_to(dest, tree)?, stats);
        Ok(())
    }

    fn copy_file_version_to(&self, dest: &Space, version: Hash, stats: &mut TransferStats) -> SpaceResult<()> {
        if dest.has_object(version)? {
            return Ok(());
        }
        let file = match self.get_object(version)? {
            Some(Object::File(f)) => f,
            _ => return Err(DuhError::object_not_found(&version.to_string(), "file").into()),
        };
        for frag_hash in &file.fragments {
            if let Some(Object::FileDiffFragment(FileFragment::ADDED { body, .. })) =
                self.get_object(*frag_hash)?
            {
                self.count(self.copy_object_to(dest, body)?, stats);
            }
            self.count(self.copy_object_to(dest, *frag_hash)?, stats);
        }
        self.count(self.copy_object_to(dest, version)?, stats);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::file::FileOps;
    use crate::space::Space;
    use crate::store;
    use std::io::Read;
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

    fn memory_store() -> store::Store {
        store::from_operator(opendal::Operator::new(opendal::services::Memory::default()).unwrap().finish())
            .unwrap()
    }

    /// The scenario spaces exist for: two spaces on different backends (a
    /// local directory and an in-memory store standing in for S3), each given
    /// its store from outside; the local space reads its objects and the
    /// remote space saves them.
    #[test]
    fn local_space_copies_history_into_a_space_on_another_backend() {
        let dir = TempDir::new().unwrap();
        let meta = dir.path().join(".duh");
        std::fs::create_dir_all(&meta).unwrap();
        let mut local = Space::init(store::local(&meta).unwrap(), Some(dir.path().into())).unwrap();
        let remote = Space::init(memory_store(), None).unwrap();

        let v1 = noise(1, 200_000);
        let mut v2 = v1.clone();
        v2.splice(100_000..100_000, noise(2, 20_000));
        let mut heads = Vec::new();
        for (path, data) in [("a.bin", &v1), ("dir/b.bin", &v1), ("a.bin", &v2)] {
            let full = dir.path().join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, data).unwrap();
            let mut ops = FileOps::from_space(&mut local);
            ops.stage_file(path.into(), None::<fn(_)>, None::<fn(_)>).unwrap();
            heads.push(ops.commit(format!("add {}", path)).unwrap());
        }
        let head = *heads.last().unwrap();

        let stats = local.copy_commit_to(&remote, head).unwrap();
        assert_eq!(stats.commits, 3);
        assert!(stats.objects > 3 && stats.bytes > 0);

        // Everything is readable from the remote space alone.
        let mut remote = remote;
        for (commit, path, want) in [(heads[0], "a.bin", &v1), (head, "dir/b.bin", &v1), (head, "a.bin", &v2)] {
            let mut got = Vec::new();
            FileOps::from_space(&mut remote)
                .open_file(path.into(), commit)
                .unwrap()
                .read_to_end(&mut got)
                .unwrap();
            assert!(got == *want, "{} at {} differs on the remote", path, commit.to_string());
        }

        // A second transfer has nothing to do.
        assert_eq!(local.copy_commit_to(&remote, head).unwrap(), Default::default());

        // Objects can also be moved one at a time, by hash.
        let other = Space::init(memory_store(), None).unwrap();
        assert!(local.copy_object_to(&other, head).unwrap() > 0);
        assert!(other.has_object(head).unwrap());
        assert_eq!(local.copy_object_to(&other, head).unwrap(), 0);
    }

    /// Solid copies follow history to the other space, including one made
    /// after its save had already been copied there.
    #[test]
    fn solid_copies_follow_history() {
        let dir = TempDir::new().unwrap();
        let meta = dir.path().join(".duh");
        std::fs::create_dir_all(&meta).unwrap();
        let mut local = Space::init(store::local(&meta).unwrap(), Some(dir.path().into())).unwrap();
        local.solid_after = 4;
        let mut remote = Space::init(memory_store(), None).unwrap();

        let mut data = noise(1, 100_000);
        let mut save = |local: &mut Space, i: u8| {
            data[i as usize * 1_000] = i;
            std::fs::write(dir.path().join("f.bin"), &data).unwrap();
            let mut ops = FileOps::from_space(local);
            ops.stage_file("f.bin".into(), None::<fn(_)>, None::<fn(_)>).unwrap();
            (ops.commit(format!("save {}", i)).unwrap(), data.clone())
        };

        // Saves 1-4: nothing solid yet. Copy them over.
        let mut saves: Vec<_> = (1..=4).map(|i| save(&mut local, i)).collect();
        local.copy_commit_to(&remote, saves[3].0).unwrap();
        assert!(remote.list_solids().unwrap().is_empty());

        // Save 5 makes save 3 solid, though remote already has save 3.
        saves.push(save(&mut local, 5));
        assert_eq!(local.list_solids().unwrap().len(), 1);
        let stats = local.copy_commit_to(&remote, saves[4].0).unwrap();
        assert_eq!(stats.commits, 1);
        assert_eq!(remote.list_solids().unwrap(), local.list_solids().unwrap());

        for (commit, want) in &saves {
            let mut got = Vec::new();
            FileOps::from_space(&mut remote)
                .open_file("f.bin".into(), *commit)
                .unwrap()
                .read_to_end(&mut got)
                .unwrap();
            assert!(got == *want);
        }
        assert_eq!(local.copy_commit_to(&remote, saves[4].0).unwrap(), Default::default());
    }

    #[test]
    fn copy_rejects_corrupted_bytes() {
        let a = Space::init(memory_store(), None).unwrap();
        let h = a.save_obj(crate::objects::Object::Fragment(crate::objects::Fragment(vec![1, 2, 3]))).unwrap();
        let mut bytes = a.read_raw_object(h).unwrap().unwrap();
        bytes.push(0);
        let b = Space::init(memory_store(), None).unwrap();
        assert!(b.write_raw_object(h, bytes).is_err());
    }
}
