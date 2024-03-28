use crate::bonsai_database::DatabaseKey;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// Key in the database of the different elements that are used in the storage of the trie data.
/// Use `new` function to create a new key.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) enum TrieKey {
    Trie(Vec<u8>),
    Flat(Vec<u8>),
}

pub(crate) enum TrieKeyType {
    Trie = 0,
    Flat = 1,
}

impl From<TrieKey> for u8 {
    fn from(value: TrieKey) -> Self {
        match value {
            TrieKey::Trie(_) => TrieKeyType::Trie as u8,
            TrieKey::Flat(_) => TrieKeyType::Flat as u8,
        }
    }
}

impl From<&TrieKey> for u8 {
    fn from(value: &TrieKey) -> Self {
        match value {
            TrieKey::Trie(_) => TrieKeyType::Trie as u8,
            TrieKey::Flat(_) => TrieKeyType::Flat as u8,
        }
    }
}

impl TrieKey {
    pub fn new(identifier: &[u8], key_type: TrieKeyType, key: &[u8]) -> Self {
        let mut final_key = identifier.to_vec();
        final_key.extend_from_slice(key);
        match key_type {
            TrieKeyType::Trie => TrieKey::Trie(final_key),
            TrieKeyType::Flat => TrieKey::Flat(final_key),
        }
    }

    pub fn from_variant_and_bytes(variant: u8, bytes: Vec<u8>) -> Self {
        match variant {
            x if x == TrieKeyType::Trie as u8 => TrieKey::Trie(bytes),
            x if x == TrieKeyType::Flat as u8 => TrieKey::Flat(bytes),
            _ => panic!("Invalid trie key type"),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            TrieKey::Trie(slice) => slice,
            TrieKey::Flat(slice) => slice,
        }
    }
}

impl<'a> From<&'a TrieKey> for DatabaseKey<'a> {
    fn from(key: &'a TrieKey) -> Self {
        let key_slice = key.as_slice();
        match key {
            TrieKey::Trie(_) => DatabaseKey::Trie(key_slice),
            TrieKey::Flat(_) => DatabaseKey::Flat(key_slice),
        }
    }
}
