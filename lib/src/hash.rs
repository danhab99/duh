use serde::{
    de::{self, SeqAccess, Visitor},
    Deserialize, Serialize,
};
use sha2::{Digest, Sha256};
use std::{error::Error, fmt};

use crate::utils::hash_string;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash([u8; 64]);

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct HashVisitor;

        impl<'de> Visitor<'de> for HashVisitor {
            type Value = Hash;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a byte array of length 64")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<Hash, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let mut array = [0u8; 64];
                for i in 0..64 {
                    array[i] = seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(i, &self))?;
                }
                Ok(Hash(array))
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Hash, E>
            where
                E: de::Error,
            {
                if v.len() == 64 {
                    let mut array = [0u8; 64];
                    array.copy_from_slice(v);
                    Ok(Hash(array))
                } else {
                    Err(de::Error::invalid_length(v.len(), &self))
                }
            }
        }

        deserializer.deserialize_bytes(HashVisitor)
    }
}

impl Serialize for Hash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

const ZERO_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000000";

impl std::string::ToString for Hash {
    fn to_string(&self) -> String {
        self.to_hex()
    }
}

impl Hash {
    pub fn from_string(s: String) -> Result<Hash, Box<dyn Error>> {
        // Expect a 64-character hex string (hex-encoded SHA-256). Store the
        // ASCII hex bytes directly in the 64-byte internal buffer.
        if s.len() != 64 {
            return Err(format!("invalid hash string length: expected 64, got {}", s.len()).into());
        }

        // Validate that it's valid hex (but don't decode — we store the ASCII).
        hex::decode(&s).map_err(|e| format!("invalid hex in hash string: {}", e))?;

        // The zero hash ("no commit" / staged deletion) is stored as NUL bytes.
        if s == ZERO_HEX {
            return Ok(Hash::new());
        }

        let mut h = [0u8; 64];
        h.copy_from_slice(s.as_bytes());
        Ok(Hash(h))
    }

    pub fn from_str(s: &str) -> Hash {
        let mut h = [0u8; 64];
        h.copy_from_slice(s.as_bytes());
        return Hash(h);
    }

    pub fn from_slice(s: &[u8]) -> Hash {
        let mut h = [0u8; 64];
        h.copy_from_slice(s);
        return Hash(h);
    }

    pub fn from_vec(v: Vec<u8>) -> Hash {
        Hash::from_slice(v.as_slice())
    }

    pub fn new() -> Hash {
        Hash([0u8; 64])
    }

    pub fn digest_string(s: String) -> Result<Hash, Box<dyn Error>> {
        Ok(Hash::from_string(hash_string(s)?)?)
    }

    pub fn digest_slice(s: &[u8]) -> Result<Hash, Box<dyn Error>> {
        let d = Sha256::digest(s);
        Ok(Hash::from_string(hex::encode(d))?)
    }

    pub fn digest_file_stream<F: std::io::Read>(f: &mut F) -> Result<Hash, Box<dyn Error>> {
        let mut h = Sha256::new();
        let mut buf = [0u8; 8 * 1024];

        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }

        let d = h.finalize();
        Ok(Hash::from_string(hex::encode(d))?)
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|x| *x == 0)
    }

    /// The 64-character hex form; the zero hash renders as all '0's.
    pub fn to_hex(&self) -> String {
        if self.is_zero() {
            return ZERO_HEX.to_string();
        }
        String::from_utf8_lossy(&self.0).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_hash_round_trips_as_hex() {
        let zero = Hash::new();
        assert_eq!(zero.to_string(), "0".repeat(64));
        assert!(Hash::from_string(zero.to_string()).unwrap().is_zero());
    }

    #[test]
    fn hash_round_trips_as_hex() {
        let h = Hash::digest_slice(b"duh").unwrap();
        assert_eq!(Hash::from_string(h.to_string()).unwrap(), h);
    }
}
