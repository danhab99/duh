use memchr::memmem;
use std::io::{Read, Seek};
use std::{fmt, io};

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    memmem::find(haystack, needle)
}

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
    if size == 0 {
        return Ok((vec![0u8; 0], false));
    }
    let mut buf = vec![0u8; size];
    let n = reader.read(&mut buf)?;

    if n == 0 {
        println!("eof");
        Ok((Vec::new(), true))
    } else {
        buf.truncate(n);
        Ok((buf, false))
    }
}

pub fn collect_divergence<R: Read + Seek>(
    old: &mut R,
    new: &mut R,
    window: usize,
) -> Result<(usize, Vec<u8>), Box<dyn std::error::Error>> {
    // deleted, added

    let old_starting_pos = old.stream_position()?;
    let new_starting_pos = new.stream_position()?;

    if window == 0 {
        return Err("window is 0".into());
    }

    // Buffer old data to search for convergence
    let mut old_data = Vec::new();
    old.read_to_end(&mut old_data)?;

    // Need at least 2 windows worth of data to find convergence
    let needle_size = window * 2;

    // Read new data incrementally, building up potential added bytes
    let mut added_bytes = Vec::new();
    let mut needle = vec![0u8; needle_size];
    let mut needle_filled = 0usize;

    loop {
        // Read one window at a time from new
        let (chunk, eof) = read_chunk(new, window)?;
        if chunk.is_empty() && eof {
            break;
        }

        // Shift needle and append new chunk
        if needle_filled >= needle_size {
            // Move the second half to the first half
            needle.copy_within(window..needle_size, 0);
            needle[window..needle_size].copy_from_slice(&chunk);
            // The bytes that fell off the front are confirmed added
            added_bytes.extend_from_slice(&needle[..window.min(chunk.len())]);
        } else {
            // Still filling the needle
            let copy_len = chunk.len().min(needle_size - needle_filled);
            needle[needle_filled..needle_filled + copy_len].copy_from_slice(&chunk[..copy_len]);
            needle_filled += copy_len;
        }

        // Once we have a full needle, try to find it in old
        if needle_filled >= needle_size {
            if let Some(pos) = find(&old_data, &needle) {
                // Found convergence point
                let deleted_bytes = pos;

                // Seek both files to after the convergence point
                old.seek(io::SeekFrom::Start(old_starting_pos + pos as u64 + needle_size as u64))?;
                // new is already positioned after the needle

                println!(
                    "success collecting divergence deleted={} added={} window={}",
                    deleted_bytes,
                    added_bytes.len(),
                    window
                );
                return Ok((deleted_bytes, added_bytes));
            }
        }

        if eof {
            break;
        }
    }

    // No convergence found, try with smaller window
    if window > 1 {
        old.seek(io::SeekFrom::Start(old_starting_pos))?;
        new.seek(io::SeekFrom::Start(new_starting_pos))?;
        println!("!!! failed to collect divergence at end window={}", window);
        return collect_divergence(old, new, window / 2);
    }

    // Smallest window, return all as divergence
    // Need to collect remaining new data
    let mut remaining = Vec::new();
    new.seek(io::SeekFrom::Start(new_starting_pos))?;
    new.read_to_end(&mut remaining)?;

    println!("no convergence found, returning all as divergence");
    Ok((old_data.len(), remaining))
}

fn collect_convergence<R: Read + Seek>(
    old: &mut R,
    new: &mut R,
    window: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
    if window < 1 {
        return Err("window is too small".into());
    }

    let old_starting_pos = old.stream_position()?;
    let new_starting_pos = new.stream_position()?;

    let mut unchanged_len = 0usize;
    let mut hit_eof = false;

    loop {
        let (old_buf, old_eof) = read_chunk(old, window)?;
        let (new_buf, new_eof) = read_chunk(new, window)?;

        if old_eof || new_eof {
            hit_eof = true;
        }

        // Direct byte comparison
        if old_buf != new_buf {
            // Found divergence, rewind to before this chunk
            old.seek(io::SeekFrom::Start(old_starting_pos + unchanged_len as u64))?;
            new.seek(io::SeekFrom::Start(new_starting_pos + unchanged_len as u64))?;
            break;
        }

        unchanged_len += old_buf.len();

        if hit_eof {
            println!("eof break");
            return Ok(unchanged_len);
        }
    }

    if unchanged_len == 0 && !hit_eof {
        // No convergence yet, but not EOF - try smaller window
        println!("!!! failed collecting convergence window={}", window);
        old.seek(io::SeekFrom::Start(old_starting_pos))?;
        new.seek(io::SeekFrom::Start(new_starting_pos))?;
        return collect_convergence(old, new, window / 2);
    }

    println!(
        "success collecting convergence window={} len={}",
        window, unchanged_len
    );
    Ok(unchanged_len)
}

fn test_eof<R: Read + Seek>(reader: &mut R) -> Result<bool, Box<dyn std::error::Error>> {
    let p = reader.stream_position()?;
    let (c, eof) = read_chunk(reader, 1)?;
    if c.len() > 0 {
        reader.seek(io::SeekFrom::Start(p))?;
    }
    println!("test eof {} {} {}", p, c.len(), eof);
    return Ok(eof);
}

pub fn build_diff_fragments<R: Read + Seek>(
    mut old: R,
    mut new: R,
    window: usize,
) -> Result<Vec<DiffFragment>, Box<dyn std::error::Error>> {
    let mut fragments = Vec::new();

    while !test_eof(&mut old)? && !test_eof(&mut new)? {
        println!("-----");
        println!("1. collecting convergence");
        let converged = collect_convergence(&mut old, &mut new, window)?;
        if converged > 0 {
            println!("added unchanged fragment {}", converged);
            fragments.push(DiffFragment::UNCHANGED { len: converged });
        }

        println!(
            "-- old converged stream position {}",
            old.stream_position()?
        );
        println!(
            "-- new converged stream position {}",
            new.stream_position()?
        );

        println!("2. collecting divergence");
        let (deleted, added) = collect_divergence(&mut old, &mut new, window)?;
        if deleted > 0 {
            println!("added deleted fragment {}", deleted);
            fragments.push(DiffFragment::DELETED { len: deleted });
        }
        if added.len() > 0 {
            println!("added added fragment {}", added.len());
            fragments.push(DiffFragment::ADDED { body: added });
        }

        println!(
            "-- old converged stream position {}",
            old.stream_position()?
        );
        println!(
            "-- new converged stream position {}",
            new.stream_position()?
        );
    }

    if !test_eof(&mut new)? {
        let mut buf = Vec::new();
        new.read_to_end(&mut buf)?;
        println!("append added {}", buf.len());
        if buf.len() > 0 {
            fragments.push(DiffFragment::ADDED { body: buf });
        }
    }

    if !test_eof(&mut old)? {
        let mut buf = Vec::new();
        old.read_to_end(&mut buf)?;
        println!("append deleted {}", buf.len());
        if buf.len() > 0 {
            fragments.push(DiffFragment::DELETED { len: buf.len() });
        }
    }

    println!("Completed with {} fragments", fragments.len());
    Ok(fragments)
}
