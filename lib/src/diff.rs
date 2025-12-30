use ahash::HashSetExt;
use rmp::decode::RmpRead;
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

pub fn collect_divergence<R: Read + Seek>(
    mut old: &mut R,
    mut new: &mut R,
    window: usize,
) -> Result<(usize, Vec<u8>), Box<dyn std::error::Error>> {
    // deleted, added
    let mut seen_hashes_set = ahash::HashSet::new();
    let mut seen_hashes_positions = Vec::<u64>::new();

    let read_chunk = |reader: &mut R| -> std::io::Result<(Vec<u8>, bool)> {
        let mut buf = vec![0u8; 32];
        let n = reader.read(&mut buf)?;

        if n == 0 {
            // EOF
            Ok((Vec::new(), true))
        } else {
            buf.truncate(n);
            Ok((buf, false))
        }
    };

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

        (old_data, old_eof) = read_chunk(&mut old)?;
        deleted_bytes += old_data.len();
        let old_hash = hash_chunk(old_data)?;
        seen_hashes_set.insert(old_hash);
        seen_hashes_positions.push(old_hash);

        (new_data, new_eof) = read_chunk(&mut new)?;
        let new_hash = hash_chunk(new_data.clone())?;
        added_bytes.append(&mut new_data);

        if seen_hashes_set.contains(&new_hash) {
            // convergence found
            converged = true;

            if let Some(convergence_position) =
                seen_hashes_positions.iter().rposition(|x| *x == new_hash)
            {
                if convergence_position == seen_hashes_positions.len() - 1 {
                    return Ok((deleted_bytes, added_bytes));
                }

                (new_data, new_eof) = read_chunk(&mut new)?;
                let next_chunk = hash_chunk(new_data)?;

                if next_chunk == seen_hashes_positions[convergence_position + 1] {
                    return Ok((deleted_bytes, added_bytes));
                } else {
                    old.seek(io::SeekFrom::Start(0))?;
                    new.seek(io::SeekFrom::Start(0))?;
                    return Ok(collect_divergence(old, new, window / 2)?);
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
            deleted_bytes += buf.len();
        }

        Ok((deleted_bytes, added_bytes))
    } else {
        Err("cannot converge".into())
    }
}

fn collect_convergence<R: Read>(old: &mut R, new: &mut R) -> io::Result<usize> {
    let mut old_hasher = blake3::Hasher::new();
    let mut new_hasher = blake3::Hasher::new();
    let mut unchanged_len = 0usize;

    loop {
        let _ = old_hasher.write_u8(old.read_u8()?);
        let _ = new_hasher.write_u8(new.read_u8()?);

        let old_hash = old_hasher.finalize();
        let new_hash = new_hasher.finalize();

        unchanged_len += 1;

        if old_hash != new_hash {
            break;
        }
    }

    Ok(unchanged_len)
}

pub fn build_diff_fragments<R: Read + Seek>(
    mut old: R,
    mut new: R,
    window: usize,
) -> Result<Vec<DiffFragment>, Box<dyn std::error::Error>> {
    let mut fragments = Vec::new();
    let mut last_len: Option<usize> = None;

    while match last_len {
        Some(l) => l < fragments.len(),
        None => true,
    } {
        let converged = collect_convergence(&mut old, &mut new)?;
        if converged > 0 {
            fragments.push(DiffFragment::UNCHANGED { len: converged });
        }

        let (deleted, added) = collect_divergence(&mut old, &mut new, window)?;
        if deleted > 0 {
            fragments.push(DiffFragment::DELETED { len: deleted });
        }
        if added.len() > 0 {
            fragments.push(DiffFragment::ADDED { body: added });
        }

        last_len = Some(fragments.len());
    }

    println!("Completed with {} fragments", fragments.len());
    Ok(fragments)
}
