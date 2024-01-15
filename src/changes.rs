use crate::id::Id;
use serde::{Deserialize, Serialize};
#[cfg(feature = "std")]
use std::collections::{hash_map::Entry, HashMap, VecDeque};
#[cfg(not(feature = "std"))]
use {
    alloc::{collections::VecDeque, vec::Vec},
    hashbrown::{hash_map::Entry, HashMap},
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Change {
    pub old_value: Option<Vec<u8>>,
    pub new_value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChangeKeyType {
    Trie(Vec<u8>),
    Flat(Vec<u8>),
}

impl ChangeKeyType {
    pub fn get_id(&self) -> u8 {
        match self {
            ChangeKeyType::Trie(_) => 0,
            ChangeKeyType::Flat(_) => 1,
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            ChangeKeyType::Trie(key) => key.as_slice(),
            ChangeKeyType::Flat(key) => key.as_slice(),
        }
    }

    pub fn from_id(id: u8, key: Vec<u8>) -> Self {
        match id {
            0 => ChangeKeyType::Trie(key),
            1 => ChangeKeyType::Flat(key),
            _ => panic!("Invalid id"),
        }
    }
}

#[derive(Debug, Default)]
pub struct ChangeBatch(pub(crate) HashMap<ChangeKeyType, Change>);

const KEY_SEPARATOR: u8 = 0x00;
const NEW_VALUE: u8 = 0x00;
const OLD_VALUE: u8 = 0x01;

impl ChangeBatch {
    pub fn insert_in_place(&mut self, key: ChangeKeyType, change: Change) {
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

    pub fn serialize<ID: Id>(&self, id: &ID) -> Vec<(Vec<u8>, &[u8])> {
        let id = id.serialize();
        self.0
            .iter()
            .flat_map(|(change_key, change)| {
                let key_slice = change_key.as_slice();
                let mut changes = Vec::new();

                if let Some(old_value) = &change.old_value {
                    let key = [
                        id.as_slice(),
                        &[KEY_SEPARATOR],
                        key_slice,
                        &[change_key.get_id()],
                        &[OLD_VALUE],
                    ]
                    .concat();
                    changes.push((key, old_value.as_slice()));
                }

                if let Some(new_value) = &change.new_value {
                    let key = [
                        id.as_slice(),
                        &[KEY_SEPARATOR],
                        key_slice,
                        &[change_key.get_id()],
                        &[NEW_VALUE],
                    ]
                    .concat();
                    changes.push((key, new_value.as_slice()));
                }
                changes
            })
            .collect()
    }

    pub fn deserialize<ID: Id>(id: &ID, changes: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
        let id = id.serialize();
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
            let change_key = ChangeKeyType::from_id(key_type, key[id.len() + 1..].to_vec());
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

pub struct ChangeStore<ID>
where
    ID: Id,
{
    // Newest are inserted at the back
    pub id_queue: VecDeque<ID>,
    pub current_changes: ChangeBatch,
}

impl<ID> ChangeStore<ID>
where
    ID: Id,
{
    pub fn new() -> Self {
        Self {
            id_queue: VecDeque::new(),
            current_changes: ChangeBatch(HashMap::new()),
        }
    }
}
