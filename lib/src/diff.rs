use std::collections::VecDeque;
use std::io::{Read, Result};

#[derive(PartialEq, Eq, Clone, Debug, serde::Deserialize, serde::Serialize)]
pub enum DiffFragment {
    ADDED { body: Vec<u8> },
    UNCHANGED { len: usize },
    DELETED { len: usize },
}

// Simple Rabin-Karp rolling hash
struct RollingHash {
    window: VecDeque<u8>,
    hash: u64,
    base: u64,
    modulus: u64,
    pow_base: u64,
}

impl RollingHash {
    fn new(base: u64, modulus: u64) -> Self {
        Self {
            window: VecDeque::new(),
            hash: 0,
            base,
            modulus,
            pow_base: 1,
        }
    }

    fn push(&mut self, byte: u8) {
        self.window.push_back(byte);
        self.hash = (self.hash * self.base + byte as u64) % self.modulus;
        if self.window.len() > 1 {
            self.pow_base = (self.pow_base * self.base) % self.modulus;
        }
    }

    fn pop(&mut self) {
        if let Some(front) = self.window.pop_front() {
            self.hash = (self.hash + self.modulus - (front as u64 * self.pow_base) % self.modulus)
                % self.modulus;
        }
    }

    fn len(&self) -> usize {
        self.window.len()
    }

    fn digest(&self) -> u64 {
        self.hash
    }

    fn current_window(&self) -> &[u8] {
        self.window.make_contiguous()
    }
}

pub fn diff_streams<R1: Read, R2: Read>(
    mut old: R1,
    mut new: R2,
    block_size: usize,
) -> Result<Vec<DiffFragment>> {
    let mut old_buffer = Vec::new();
    let mut new_buffer = Vec::new();

    old.read_to_end(&mut old_buffer)?;
    new.read_to_end(&mut new_buffer)?;

    let mut diffs = Vec::new();
    let mut i = 0;
    let mut j = 0;

    while i < old_buffer.len() && j < new_buffer.len() {
        let end_i = usize::min(i + block_size, old_buffer.len());
        let end_j = usize::min(j + block_size, new_buffer.len());

        let old_block = &old_buffer[i..end_i];
        let new_block = &new_buffer[j..end_j];

        if old_block == new_block {
            diffs.push(DiffFragment::UNCHANGED {
                len: old_block.len(),
            });
            i += old_block.len();
            j += new_block.len();
        } else {
            // find the longest match using rolling hash
            let mut found_match = false;
            let rh_old = &old_buffer[i..end_i];
            for k in 0..new_block.len() {
                let sub = &new_block[k..];
                if sub.starts_with(rh_old) {
                    if k > 0 {
                        diffs.push(DiffFragment::ADDED {
                            body: new_block[..k].to_vec(),
                        });
                    }
                    diffs.push(DiffFragment::UNCHANGED { len: rh_old.len() });
                    i += rh_old.len();
                    j += k + rh_old.len();
                    found_match = true;
                    break;
                }
            }
            if !found_match {
                diffs.push(DiffFragment::DELETED {
                    len: old_block.len(),
                });
                diffs.push(DiffFragment::ADDED {
                    body: new_block.to_vec(),
                });
                i += old_block.len();
                j += new_block.len();
            }
        }
    }

    if i < old_buffer.len() {
        diffs.push(DiffFragment::DELETED {
            len: old_buffer.len() - i,
        });
    }
    if j < new_buffer.len() {
        diffs.push(DiffFragment::ADDED {
            body: new_buffer[j..].to_vec(),
        });
    }

    Ok(diffs)
}
