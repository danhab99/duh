use serde::{
    de::{self, SeqAccess, Visitor},
    Deserialize, Serialize,
};
use std::fmt;

#[derive(Debug)]
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

impl std::string::ToString for Hash {
    fn to_string(&self) -> String {
        let v = &self.0;
        String::from_utf8(v.to_vec()).unwrap()
    }
}

impl Hash {
    pub fn from_string(s: String) -> Hash {
        let mut h = [0u8; 64];
        h.copy_from_slice(s.as_bytes());
        return Hash(h);
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

    pub fn new() -> Hash {
        Hash([0u8; 64])
    }
}
