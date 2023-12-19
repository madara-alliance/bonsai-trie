use crate::{bonsai_database::KeyType, changes::ChangeKeyType};

#[derive(Debug, Hash, PartialEq, Eq)]
pub enum TrieKeyType {
    Trie(Vec<u8>),
    Flat(Vec<u8>),
}

impl TrieKeyType {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            TrieKeyType::Trie(slice) => slice,
            TrieKeyType::Flat(slice) => slice,
        }
    }
}

impl<'a> From<&'a TrieKeyType> for KeyType<'a> {
    fn from(key: &'a TrieKeyType) -> Self {
        let key_slice = key.as_slice();
        match key {
            TrieKeyType::Trie(_) => KeyType::Trie(key_slice),
            TrieKeyType::Flat(_) => KeyType::Flat(key_slice),
        }
    }
}

impl<'a> From<&'a TrieKeyType> for ChangeKeyType {
    fn from(key: &'a TrieKeyType) -> Self {
        let key_slice = key.as_slice();
        match key {
            TrieKeyType::Trie(_) => ChangeKeyType::Trie(key_slice.to_vec()),
            TrieKeyType::Flat(_) => ChangeKeyType::Flat(key_slice.to_vec()),
        }
    }
}
