use crate::{hash_map::Entry, id::Id, trie::TrieKey, ByteVec, HashMap, Vec};
use core::iter;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Change {
    pub old_value: Option<ByteVec>,
    pub new_value: Option<ByteVec>,
}

#[derive(Debug, Default)]
#[cfg_attr(feature = "bench", derive(Clone))]
pub struct ChangeBatch(pub(crate) HashMap<TrieKey, Change>);

const KEY_SEPARATOR: u8 = 0x00;
const NEW_VALUE: u8 = 0x00;
const OLD_VALUE: u8 = 0x01;

impl ChangeBatch {
    pub fn insert_in_place(&mut self, key: TrieKey, change: Change) {
        match self.0.entry(key) {
            Entry::Occupied(mut entry) => {
                let e = entry.get_mut();
                if e.old_value.is_none() {
                    e.old_value = change.old_value;
                }
                e.new_value = change.new_value;
            }
            Entry::Vacant(entry) => {
                entry.insert(change);
            }
        }
    }

    pub fn serialize<ID: Id>(&self, id: &ID) -> Vec<(ByteVec, &[u8])> {
        self.0
            .iter()
            .flat_map(|(change_key, change)| {
                let mut changes = Vec::new();

                if let Some(old_value) = &change.old_value {
                    if let Some(new_value) = &change.new_value {
                        if old_value == new_value {
                            return changes;
                        }
                    }
                    let key = key_old_value(id, change_key);
                    changes.push((key, old_value.as_slice()));
                }

                if let Some(new_value) = &change.new_value {
                    let key = key_new_value(id, change_key);
                    changes.push((key, new_value.as_slice()));
                }
                changes
            })
            .collect()
    }

    pub fn deserialize<ID: Id>(id: &ID, changes: Vec<(ByteVec, ByteVec)>) -> Self {
        let id = id.to_bytes();
        let mut change_batch = ChangeBatch(HashMap::new());
        let mut current_change = Change::default();
        let mut last_key = None;
        for (key, value) in changes {
            if key.len() < id.len() + 3 {
                panic!("Invalid key format");
            }
            // following unwraps and indices are safe because of the check above
            let mut key = key.to_vec();
            let change_type = key.pop().unwrap();
            let key_type = key.pop().unwrap();
            let change_key = TrieKey::from_variant_and_bytes(key_type, key[id.len() + 1..].into());
            if let Some(last_key) = last_key {
                if last_key != change_key {
                    change_batch.insert_in_place(last_key, current_change);
                    current_change = Change::default();
                }
            }
            match change_type {
                NEW_VALUE => current_change.new_value = Some(value),
                OLD_VALUE => current_change.old_value = Some(value),
                _ => panic!("Invalid change type"),
            }
            last_key = Some(change_key.clone());
        }
        if let Some(last_key) = last_key {
            if current_change.new_value.is_some() || current_change.old_value.is_some() {
                change_batch.insert_in_place(last_key, current_change);
            }
        }
        change_batch
    }
}

pub fn key_old_value<ID: Id>(id: &ID, key: &TrieKey) -> ByteVec {
    id.to_bytes()
        .into_iter()
        .chain(iter::once(KEY_SEPARATOR))
        .chain(key.as_slice().iter().copied())
        .chain(iter::once(key.into()))
        .chain(iter::once(OLD_VALUE))
        .collect()
}

pub fn key_new_value<ID: Id>(id: &ID, key: &TrieKey) -> ByteVec {
    id.to_bytes()
        .into_iter()
        .chain(iter::once(KEY_SEPARATOR))
        .chain(key.as_slice().iter().copied())
        .chain(iter::once(key.into()))
        .chain(iter::once(NEW_VALUE))
        .collect()
}

#[cfg_attr(feature = "bench", derive(Clone))]
#[derive(Debug)]
pub struct ChangeStore {
    pub current_changes: ChangeBatch,
}

impl ChangeStore {
    pub fn new() -> Self {
        Self {
            current_changes: ChangeBatch(HashMap::new()),
        }
    }
}
