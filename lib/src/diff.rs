use ahash::HashSetExt;
use rmp::encode::RmpWrite;
use std::io::{Read, Seek};
use std::{fmt, io};
use xxhash_rust::xxh3::Xxh3;

#[derive(PartialEq, Eq, Clone, Debug, serde::Deserialize, serde::Serialize)]
pub enum DiffFragment {
    ADDED { body: Vec<u8> },
    UNCHANGED { len: usize },
    DELETED { len: usize },
}

impl fmt::Display for DiffFragment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiffFragment::ADDED { body } => write!(f, "+{}", body.len()),
            DiffFragment::UNCHANGED { len } => write!(f, "={}", len),
            DiffFragment::DELETED { len } => write!(f, "-{}", len),
        }
    }
}

fn read_chunk<R: Read>(reader: &mut R, size: usize) -> std::io::Result<(Vec<u8>, bool)> {
    let mut buf = vec![0u8; size];
    let n = reader.read(&mut buf)?;

    if n == 0 {
        // EOF
        Ok((Vec::new(), true))
    } else {
        buf.truncate(n);
        Ok((buf, false))
    }
}

pub fn collect_divergence<R: Read + Seek>(
    mut old: &mut R,
    mut new: &mut R,
    window: usize,
) -> Result<(usize, Vec<u8>), Box<dyn std::error::Error>> {
    // deleted, added

    if window == 0 {
        return Err("window is 0".into());
    }

    let mut seen_hashes_set = ahash::HashSet::new();
    let mut seen_hashes_positions = Vec::<u64>::new();

    let hash_chunk = |v: Vec<u8>| -> Result<_, Box<dyn std::error::Error>> {
        let mut h = Xxh3::new();
        h.write_bytes(v.as_slice())?;
        Ok(h.digest())
    };

    let mut old_eof: bool;
    let mut new_eof: bool;
    let mut converged = false;

    let mut deleted_bytes = 0usize;
    let mut added_bytes = Vec::<u8>::new();

    loop {
        let mut old_data = vec![0u8; window];
        let mut new_data = vec![0u8; window];

        (old_data, old_eof) = read_chunk(&mut old, 32)?;
        let old_hash = hash_chunk(old_data)?;
        seen_hashes_set.insert(old_hash);
        seen_hashes_positions.push(old_hash);

        (new_data, new_eof) = read_chunk(&mut new, 32)?;
        let new_hash = hash_chunk(new_data.clone())?;
        added_bytes.append(&mut new_data);

        if seen_hashes_set.contains(&new_hash) && seen_hashes_set.len() > 1 {
            // convergence found
            converged = true;

            if let Some(convergence_position) =
                seen_hashes_positions.iter().rposition(|x| *x == new_hash)
            {

                (new_data, new_eof) = read_chunk(&mut new, 32)?;
                let next_chunk = hash_chunk(new_data)?;

                if convergence_position < seen_hashes_positions.len() - 2
                    && next_chunk == seen_hashes_positions[convergence_position + 1]
                {
                    deleted_bytes = convergence_position * window;
                    return Ok((deleted_bytes, added_bytes));
                }
            }
        }

        if old_eof || new_eof {
            break;
        }
    }

    if converged {
        if old_eof {
            let mut buf = Vec::new();
            new.read_to_end(&mut buf)?;
            added_bytes.append(&mut buf);
        } else if new_eof {
            let mut buf = Vec::new();
            old.read_to_end(&mut buf)?;
            deleted_bytes = buf.len() * (seen_hashes_set.len() * window);
        }

        Ok((deleted_bytes, added_bytes))
    } else {
        old.seek(io::SeekFrom::Start(0))?;
        new.seek(io::SeekFrom::Start(0))?;
        println!("failed to collect divergence at end window={}", window);
        return collect_divergence(old, new, window / 2);
    }
}

fn collect_convergence<R: Read>(
    old: &mut R,
    new: &mut R,
    window: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
    if window <= 4 {
        return Err("window is 4".into());
    }

    let mut old_hasher = blake3::Hasher::new();
    let mut new_hasher = blake3::Hasher::new();
    let mut unchanged_len = 0usize;

    loop {
        let (old_buf, old_eof) = read_chunk(old, window)?;
        let _ = old_hasher.write_bytes(old_buf.as_slice());

        let (new_buf, new_eof) = read_chunk(new, window)?;
        let _ = new_hasher.write_bytes(new_buf.as_slice());

        let old_hash = old_hasher.finalize();
        let new_hash = new_hasher.finalize();

        unchanged_len += window;

        if old_eof || new_eof {
            break;
        }
        if old_hash != new_hash {
            break;
        }
    }

    if unchanged_len <= window {
        return collect_convergence(old, new, window / 2);
    }

    Ok(unchanged_len)
}

pub fn build_diff_fragments<R: Read + Seek>(
    mut old: R,
    mut new: R,
    window: usize,
) -> Result<Vec<DiffFragment>, Box<dyn std::error::Error>> {
    let mut fragments = Vec::new();
    let mut added_fragment = true;

    while added_fragment {
        added_fragment = false;
        let converged = collect_convergence(&mut old, &mut new, window)?;
        if converged > 0 {
            fragments.push(DiffFragment::UNCHANGED { len: converged });
            added_fragment = true;
        }

        let (deleted, added) = collect_divergence(&mut old, &mut new, window)?;
        if deleted > 0 {
            fragments.push(DiffFragment::DELETED { len: deleted });
            added_fragment = true;
        }
        if added.len() > 0 {
            fragments.push(DiffFragment::ADDED { body: added });
            added_fragment = true;
        }
    }

    println!("Completed with {} fragments", fragments.len());
    Ok(fragments)
}
