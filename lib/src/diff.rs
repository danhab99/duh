use ahash::{HashMap, HashMapExt};
use rmp::encode::RmpWrite;
use std::io::{Read, Seek};
use std::{fmt, io};

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
    //
    let old_starting_pos = old.stream_position()?;
    let new_starting_pos = new.stream_position()?;

    // let mut index: HashMap<blake3::Hash, (usize, Vec<u8>)> = HashMap::new();
    let mut index: HashMap<blake3::Hash, usize> = HashMap::new();
    let mut old_total_read_bytes: usize = 0;
    let mut new_chunk_buffer: Vec<u8> = Vec::new();
    let mut converged_position: Option<usize> = None;

    loop {
        let (old_chunk, old_eof) = read_chunk(old, window)?;
        if old_chunk.is_empty() && old_eof {
            break;
        }

        old_total_read_bytes += old_chunk.len();

        let old_hash = {
            let mut h = blake3::Hasher::new();
            h.write_bytes(&old_chunk.clone())?;
            h.finalize()
        };

        index.insert(old_hash, old_total_read_bytes);

        let (mut new_chunk, new_eof) = read_chunk(new, window)?;
        if new_chunk.is_empty() && new_eof {
            break;
        }
        let new_hash = {
            let mut h = blake3::Hasher::new();
            h.write_bytes(&new_chunk)?;
            h.finalize()
        };

        match index.get(&new_hash) {
            Some(old_position_bytes) => {
                let deleted = old_position_bytes - window;
                
                let old_seek_to = old_starting_pos + *old_position_bytes as u64;
                let new_seek_to = new_starting_pos + new_chunk_buffer.len() as u64 + window as u64;
                
                println!("MATCH: deleted={} added={} window={}", deleted, new_chunk_buffer.len(), window);
                
                // Seek to after the matched chunk
                old.seek(io::SeekFrom::Start(old_seek_to))?;
                new.seek(io::SeekFrom::Start(new_seek_to))?;
                
                return Ok((deleted, new_chunk_buffer));
            }
            None => {}
        }

        // Only append after checking - matched chunk should NOT be in added
        new_chunk_buffer.append(&mut new_chunk);

        if old_eof || new_eof {
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
    
    old.seek(io::SeekFrom::Start(old_starting_pos))?;
    new.seek(io::SeekFrom::Start(new_starting_pos))?;
    
    let mut remaining_old = Vec::new();
    let mut remaining_new = Vec::new();
    old.read_to_end(&mut remaining_old)?;
    new.read_to_end(&mut remaining_new)?;
    
    println!("NO CONVERGENCE POSSIBLE: remaining old={} new={}", remaining_old.len(), remaining_new.len());
    Ok((remaining_old.len(), remaining_new))
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

    if unchanged_len == 0 {
        // Files diverge immediately at this position - that's fine, return 0
        println!("convergence=0 at window={}", window);
        old.seek(io::SeekFrom::Start(old_starting_pos))?;
        new.seek(io::SeekFrom::Start(new_starting_pos))?;
        return Ok(0);
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
