use rmp::decode::RmpRead;
use rmp::encode::RmpWrite;
use std::collections::HashMap;
use std::io::Read;
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

const WINDOW: usize = 32;
const CHUNK: usize = 4096;

fn read_more<R: Read>(r: &mut R, buf: &mut Vec<u8>) -> io::Result<()> {
    let mut tmp = [0u8; CHUNK];
    let n = r.read(&mut tmp)?;
    if n > 0 {
        buf.extend_from_slice(&tmp[..n]);
    }
    Ok(())
}

fn hash(bytes: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    let mut h = DefaultHasher::new();
    h.write(bytes);
    h.finish()
}

fn verify<R1: Read, R2: Read>(a: &mut R1, b: &mut R2) -> io::Result<bool> {
    let mut buf_a = [0u8; 1024];
    let mut buf_b = [0u8; 1024];

    loop {
        let na = a.read(&mut buf_a)?;
        let nb = b.read(&mut buf_b)?;

        if na == 0 || nb == 0 {
            return Ok(na == nb);
        }

        if buf_a[..na] != buf_b[..nb] {
            return Ok(false);
        }
    }
}

#[derive(Debug)]
pub struct Convergence {
    pub divergent_a: Vec<u8>,
    pub divergent_b: Vec<u8>,
    pub converge_a_offset: u64,
    pub converge_b_offset: u64,
}

pub fn find_convergence<R1: Read, R2: Read>(mut a: R1, mut b: R2) -> io::Result<Option<Convergence>> {
    let mut buf_a = Vec::new();
    let mut buf_b = Vec::new();

    let mut divergent_a = Vec::new();
    let mut divergent_b = Vec::new();

    let mut offset_a: u64 = 0;
    let mut offset_b: u64 = 0;

    // hash -> offset in A
    let mut table: HashMap<u64, u64> = HashMap::new();

    loop {
        let len_before_a = buf_a.len();
        let len_before_b = buf_b.len();
        
        read_more(&mut a, &mut buf_a)?;
        read_more(&mut b, &mut buf_b)?;
        
        // If no new data was read from either stream, we're at EOF
        if buf_a.len() == len_before_a && buf_b.len() == len_before_b {
            // Can't make progress anymore, return None
            println!("  EOF reached: divergent_a={}, divergent_b={}", divergent_a.len(), divergent_b.len());
            return Ok(None);
        }

        while buf_a.len() >= WINDOW {
            let hash = hash(&buf_a[..WINDOW]);
            table.entry(hash).or_insert(offset_a);

            divergent_a.push(buf_a[0]);
            buf_a.drain(..1);
            offset_a += 1;
        }

        while buf_b.len() >= WINDOW {
            let hash = hash(&buf_b[..WINDOW]);

            if let Some(&a_pos) = table.get(&hash) {
                if verify(&mut a, &mut b)? {
                    return Ok(Some(Convergence {
                        divergent_a,
                        divergent_b,
                        converge_a_offset: a_pos,
                        converge_b_offset: offset_b,
                    }));
                }
            }

            divergent_b.push(buf_b[0]);
            buf_b.drain(..1);
            offset_b += 1;
        }
    }
}

fn collect_while_converging<R: Read>(mut old: R, mut new: R) -> io::Result<usize> {
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
    mut old: R,
    mut new: R,
    block_size: usize,
) -> io::Result<Vec<DiffFragment>> {
    type Buffer = Vec<u8>;

    let mut fragments = Vec::new();

    let mut changing = false;
    let mut last_len = 1;
    let mut iteration = 0;

    println!("Starting build_diff_fragments with block_size={}", block_size);

    while last_len > 0 {
        iteration += 1;
        println!("Iteration {}: changing={}", iteration, changing);
        
        last_len = if changing {
            // collect_unchanged
            println!("  Collecting unchanged bytes...");
            let unchanged = collect_while_converging(&mut old, &mut new)?;
            println!("  Found {} unchanged bytes", unchanged);
            fragments.push(DiffFragment::UNCHANGED { len: unchanged });
            unchanged
        } else {
            // collected_changed
            println!("  Collecting changed bytes...");
            let mut buf_old = vec![0u8; block_size];
            let n_old = old.read(&mut buf_old)?;
            buf_old.truncate(n_old);
            println!("  Read {} bytes from old", n_old);

            let mut buf_new = vec![0u8; block_size];
            let n_new = new.read(&mut buf_new)?;
            buf_new.truncate(n_new);
            println!("  Read {} bytes from new", n_new);

            // If both are at EOF, we're done
            if n_old == 0 && n_new == 0 {
                println!("  Both at EOF");
                0
            } else if n_old == 0 {
                // Old is done, rest of new is additions
                println!("  Old at EOF, adding {} new bytes", n_new);
                fragments.push(DiffFragment::ADDED { body: buf_new });
                n_new
            } else if n_new == 0 {
                // New is done, rest of old is deletions
                println!("  New at EOF, deleting {} old bytes", n_old);
                fragments.push(DiffFragment::DELETED { len: n_old });
                n_old
            } else {
                match find_convergence(buf_old.as_slice(), buf_new.as_slice())? {
                    Some(convergence) => {
                        println!("  Convergence found: divergent_a={}, divergent_b={}, converge_a_offset={}, converge_b_offset={}", 
                            convergence.divergent_a.len(), convergence.divergent_b.len(),
                            convergence.converge_a_offset, convergence.converge_b_offset);

                        let div_a_len = convergence.divergent_a.len();
                        let div_b_len = convergence.divergent_b.len();

                        if div_a_len > 0 {
                            fragments.push(DiffFragment::DELETED {
                                len: div_a_len,
                            });
                            println!("  Added DELETED fragment: {} bytes", div_a_len);
                        }
                        if div_b_len > 0 {
                            fragments.push(DiffFragment::ADDED {
                                body: convergence.divergent_b,
                            });
                            println!("  Added ADDED fragment: {} bytes", div_b_len);
                        }

                        // Return the number of bytes processed (divergent bytes indicate we made progress)
                        div_a_len.max(div_b_len).max(1)
                    }
                    None => {
                        println!("  No convergence found in this block");
                        // No convergence found, treat entire buffers as changed
                        if n_old > 0 {
                            fragments.push(DiffFragment::DELETED { len: n_old });
                            println!("  Added DELETED fragment: {} bytes", n_old);
                        }
                        if n_new > 0 {
                            fragments.push(DiffFragment::ADDED { body: buf_new });
                            println!("  Added ADDED fragment: {} bytes", n_new);
                        }
                        // Return 0 to indicate no convergence, will toggle mode
                        0
                    }
                }
            }
        };
        println!("  last_len={}", last_len);

        changing = !changing;
        println!("  Toggling mode to changing={}", changing);
    }

    println!("Completed with {} fragments", fragments.len());
    Ok(fragments)
}
