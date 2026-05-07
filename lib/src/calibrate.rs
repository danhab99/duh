use crate::repo::Repo;
use std::{collections::HashMap, fs};

pub struct Calibration {
    pub chunk_size: usize,
    pub max_size: usize,
}

/**
 * best way to calibrate the algo
 *
 * you want to output atleast 100 chunks
 *
 * cdc chunking works by detecting the bottom length of continuous 1s in the binary of the hash
 *
 * count the number of trailing 1s and put it into a histogram, if 100 is the top then that is
 * the settings
 *
 * determine the window size by recursivly dividing the file in 2 until you have the potential
 * of 100 chunks
 *
 */

fn get_len_of_trailing_1s(bytes: &[u8]) -> u64 {
    let mut pos = 0u64;
    for byte in bytes.iter().rev() {
        for i in 1..8 {
            if byte & (1 << i) == 1 {
                pos += 1;
            } else {
                return pos;
            }
        }
    }

    pos
}

// returns number of largest bucket representing highest chunking, return key length and number of
// potential chunks

fn build_histogram(chunks: Vec<&[u8]>) -> (u64, u64) {
    if chunks.len() == 0 {
        return (0, 0);
    }

    let mut histogram = HashMap::<u64, u64>::new();

    chunks.iter().for_each(|chunk| {
        let h = blake3::hash(&chunk);

        let trailing = get_len_of_trailing_1s(h.as_bytes());

        let n = histogram.get(&trailing).unwrap_or(&0u64) + 1;
        histogram.insert(trailing, n);
    });

    let mut reverse_histogram: HashMap<u64, Vec<u64>> = HashMap::new();
    for (trailing, count) in &histogram {
        reverse_histogram.entry(*count).or_default().push(*trailing);
    }

    let largest_bucket = *histogram.values().max().unwrap();
    let ideal_trailing = reverse_histogram.get(&largest_bucket).unwrap();

    return (largest_bucket, *ideal_trailing.first().unwrap());
}

const RECOMMENDED_NUMBER_OF_CHUNKS: usize = 100;

fn find_ideal_window(data: &[u8]) {
    for i in 1..RECOMMENDED_NUMBER_OF_CHUNKS {
        let chunks = data
            .chunks(data.len() / i as usize)
            .collect::<Vec<_>>();

        let (potential_chunks, ideal_window) = build_histogram(chunks);


    }
}

pub fn calibrate_file(path: &str) {
    let _ = fs::File::open(path);
}
