use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Result, Write};
use std::rc::Rc;
use std::slice::Iter;
use std::{fmt, io, iter};

use rmp::decode::RmpRead;
use rmp::encode::RmpWrite;

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

pub fn collect_until_converging<R: Read>(old: R, new: R) -> Result<(usize, Vec<u8>)> {
    // deleted, added
    const CHUNK_SIZE: usize = 32usize;

    let iter_chunk = |mut reader: R| {
        iter::from_fn(move || {
            let mut buf = vec![0u8; CHUNK_SIZE];
            reader.read(&mut buf).unwrap();

            if buf.len() > 0 {
                Some(buf)
            } else {
                None
            }
        })
    };

    let iter_hashes = |it: Box<dyn Iterator<Item = Vec<u8>>>| {
        it.map(|x: Vec<u8>| (x.clone(), xxhash_rust::xxh3::xxh3_64(x.as_slice())))
    };

    let old_chunks = iter_hashes(Box::new(iter_chunk(old))).collect::<Vec<_>>();

    let mut matched_old_index = None;
    let mut collected_new_chunks = Vec::new();

    for (data, new_hash) in iter_hashes(Box::new(iter_chunk(new))) {
        if let Some(pos) = old_chunks
            .iter()
            .position(|(_, old_hash)| *old_hash == new_hash)
        {
            matched_old_index = Some(pos);
            break;
        }

        collected_new_chunks.push(data);
    }

    let confident_matched_old_index = match matched_old_index {
        None => {
            panic!("how is this possible")
        }
        Some(x) => x,
    };

    let mut total_chunks: Vec<u8> = Vec::new();
    for mut c in collected_new_chunks {
        total_chunks.append(&mut c);
    }

    Ok((confident_matched_old_index, total_chunks))
}

fn collect_while_converging<R: Read>(mut old: R, mut new: R) -> Result<usize> {
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

pub fn build_diff_fragments<R: Read>(
    old: R,
    new: R,
    block_size: usize,
) -> io::Result<Vec<DiffFragment>> {
    type Buffer = Vec<u8>;

    let read_buffer = |reader: &mut R| -> io::Result<Buffer> {
        let mut buf = vec![0u8; block_size];
        let n = reader.read(&mut buf)?;
        buf.truncate(n);
        Ok(buf)
    };

    let fragments = Rc::new(RefCell::new(Vec::new()));

    let old_rc = Rc::new(RefCell::new(old));
    let new_rc = Rc::new(RefCell::new(new));

    let collect_unchanged = {
        let fragments = Rc::clone(&fragments);
        let old_clone = Rc::clone(&old_rc);
        let new_clone = Rc::clone(&new_rc);
        move || -> Result<usize> {
            let mut old_ref = old_clone.borrow_mut();
            let mut new_ref = new_clone.borrow_mut();
            let unchanged = collect_while_converging(&mut *old_ref, &mut *new_ref)?;
            fragments
                .borrow_mut()
                .push(DiffFragment::UNCHANGED { len: unchanged });
            Ok(unchanged)
        }
    };

    let collected_changed = {
        let fragments = Rc::clone(&fragments);
        let old_clone = Rc::clone(&old_rc);
        let new_clone = Rc::clone(&new_rc);
        move || -> io::Result<usize> {
            let mut old_ref = old_clone.borrow_mut();
            let mut new_ref = new_clone.borrow_mut();
            let old_buffer = read_buffer(&mut *old_ref)?;
            let new_buffer = read_buffer(&mut *new_ref)?;
            let (deleted, added) =
                collect_until_converging(old_buffer.as_slice(), new_buffer.as_slice())?;

            let al = added.len();

            if deleted > 0 {
                fragments
                    .borrow_mut()
                    .push(DiffFragment::DELETED { len: deleted });
            }
            if added.len() > 0 {
                fragments
                    .borrow_mut()
                    .push(DiffFragment::ADDED { body: added });
            }

            return Ok(deleted + al);
        }
    };

    let mut changing = false;
    let mut last_len = 1;

    while last_len > 0 {
        last_len = if changing {
            collect_unchanged()?
        } else {
            collected_changed()?
        };

        if last_len == 0 {
            changing = !changing;
        }
    }

    Ok(Rc::try_unwrap(fragments)
        .expect("Multiple references to fragments")
        .into_inner())
}
