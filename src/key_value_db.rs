use crate::{
    changes::key_new_value, format, trie::merkle_tree::bytes_to_bitvec, BTreeSet, ByteVec,
    Change as ExternChange, ToString,
};
use bitvec::{order::Msb0, vec::BitVec};
use hashbrown::HashMap;
use log::trace;
use parity_scale_codec::Decode;
use starknet_types_core::felt::Felt;

use crate::{
    bonsai_database::{BonsaiDatabase, BonsaiPersistentDatabase, DatabaseKey},
    changes::{Change, ChangeBatch, ChangeStore},
    id::Id,
    trie::TrieKey,
    BonsaiStorageConfig, BonsaiStorageError,
};

/// Crate Trie <= KeyValueDB => BonsaiDatabase
#[cfg_attr(feature = "bench", derive(Clone))]
pub struct KeyValueDB<DB, ID>
where
    DB: BonsaiDatabase,
    ID: Id,
{
    pub(crate) db: DB,
    pub(crate) changes_store: ChangeStore<ID>,
    pub(crate) snap_holder: BTreeSet<ID>,
    pub(crate) snap_counter: u64,
    pub(crate) config: KeyValueDBConfig,
    pub(crate) created_at: Option<ID>,
}

#[derive(Clone)]
pub struct KeyValueDBConfig {
    /// Maximum number of trie logs to keep in the database (None = unlimited).
    pub max_saved_trie_logs: Option<usize>,
    /// Maximum number of snapshots to keep in the database (None = unlimited).
    pub max_saved_snapshots: Option<usize>,
    /// Interval of commit between two snapshots creation.
    pub snapshot_interval: u64,
}

impl Default for KeyValueDBConfig {
    fn default() -> Self {
        Self {
            max_saved_trie_logs: None,
            max_saved_snapshots: None,
            snapshot_interval: 5,
        }
    }
}

impl From<BonsaiStorageConfig> for KeyValueDBConfig {
    fn from(value: BonsaiStorageConfig) -> Self {
        Self {
            max_saved_trie_logs: value.max_saved_trie_logs,
            snapshot_interval: value.snapshot_interval,
            max_saved_snapshots: value.max_saved_snapshots,
        }
    }
}

impl From<KeyValueDBConfig> for BonsaiStorageConfig {
    fn from(val: KeyValueDBConfig) -> Self {
        BonsaiStorageConfig {
            max_saved_trie_logs: val.max_saved_trie_logs,
            snapshot_interval: val.snapshot_interval,
            max_saved_snapshots: val.max_saved_snapshots,
        }
    }
}

impl<DB, ID> KeyValueDB<DB, ID>
where
    DB: BonsaiDatabase,
    ID: Id,
{
    pub(crate) fn new(underline_db: DB, config: KeyValueDBConfig, created_at: Option<ID>) -> Self {
        let mut changes_store = ChangeStore::new();
        if let Some(created_at) = created_at {
            changes_store.id_queue.push_back(created_at);
        }
        Self {
            db: underline_db,
            changes_store,
            snap_holder: BTreeSet::new(),
            snap_counter: 0,
            config,
            created_at,
        }
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn get_changes(
        &self,
        id: ID,
    ) -> Result<HashMap<BitVec<u8, Msb0>, ExternChange>, BonsaiStorageError<DB::DatabaseError>>
    {
        if self.changes_store.id_queue.contains(&id) {
            let mut leaf_changes = HashMap::new();
            let changes = ChangeBatch::deserialize(
                &id,
                self.db
                    .get_by_prefix(&DatabaseKey::TrieLog(&id.to_bytes()))?,
            );
            for (k, v) in changes.0 {
                if let TrieKey::Flat(k) = k {
                    leaf_changes.insert(
                        bytes_to_bitvec(&k),
                        ExternChange {
                            // SAFETY: We are sure that the values are valid Felt because they can be saved only by our crate
                            old_value: v.old_value.map(|x| Felt::decode(&mut x.as_ref()).unwrap()),
                            new_value: v.new_value.map(|x| Felt::decode(&mut x.as_ref()).unwrap()),
                        },
                    );
                }
            }
            Ok(leaf_changes)
        } else {
            Err(BonsaiStorageError::GoTo(
                "ID asked isn't in our ID records".to_string(),
            ))
        }
    }

    pub(crate) fn commit(&mut self, id: ID) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        if Some(&id) > self.changes_store.id_queue.back() {
            self.changes_store.id_queue.push_back(id);
        } else {
            return Err(BonsaiStorageError::GoTo(format!(
                "Commit id {:?} is not greater than the last recorded id",
                id,
            )));
        }

        // Insert flat db changes
        let mut batch = self.db.create_batch();
        let current_changes = core::mem::take(&mut self.changes_store.current_changes);
        for (key, change) in current_changes.serialize(&id).iter() {
            self.db
                .insert(&DatabaseKey::TrieLog(key), change, Some(&mut batch))?;
        }
        self.db.write_batch(batch)?;

        if let Some(max_saved_trie_logs) = self.config.max_saved_trie_logs {
            while self.changes_store.id_queue.len() > max_saved_trie_logs {
                // verified by previous conditional statement
                let id = self.changes_store.id_queue.pop_front().unwrap();
                self.db
                    .remove_by_prefix(&DatabaseKey::TrieLog(&id.to_bytes()))?;
            }
        }
        Ok(())
    }

    pub(crate) fn create_batch(&self) -> DB::Batch {
        self.db.create_batch()
    }

    pub(crate) fn get_config(&self) -> KeyValueDBConfig {
        self.config.clone()
    }

    pub(crate) fn get(
        &self,
        key: &TrieKey,
    ) -> Result<Option<ByteVec>, BonsaiStorageError<DB::DatabaseError>> {
        trace!("Getting from KeyValueDB: {:?}", key);
        Ok(self.db.get(&key.into())?)
    }

    pub(crate) fn get_at(
        &self,
        key: &TrieKey,
        id: ID,
    ) -> Result<Option<ByteVec>, BonsaiStorageError<DB::DatabaseError>> {
        trace!("Getting from KeyValueDB: {:?} at ID: {:?}", key, id);

        // makes sure given id exists
        let Ok(id_position) = self.changes_store.id_queue.binary_search(&id) else {
            return Err(BonsaiStorageError::Transaction(format!(
                "invalid id {:?}",
                id
            )));
        };

        // looking for the first storage insertion with given key
        let iter = self
            .changes_store
            .id_queue
            .iter()
            .take(id_position + 1)
            .rev();
        for id in iter {
            let key = key_new_value(id, key);
            if let Some(value) = self.db.get(&DatabaseKey::TrieLog(&key))? {
                return Ok(Some(value));
            }
        }

        Ok(None)
    }

    pub(crate) fn get_latest_id(&self) -> Option<ID> {
        self.changes_store.id_queue.back().cloned()
    }

    pub(crate) fn contains(
        &self,
        key: &TrieKey,
    ) -> Result<bool, BonsaiStorageError<DB::DatabaseError>> {
        trace!("Contains from KeyValueDB: {:?}", key);
        Ok(self.db.contains(&key.into())?)
    }

    pub(crate) fn insert(
        &mut self,
        key: &TrieKey,
        value: &[u8],
        batch: Option<&mut DB::Batch>,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        trace!("Inserting into KeyValueDB: {:?} {:?}", key, value);
        let old_value = self.db.insert(&key.into(), value, batch)?;
        self.changes_store.current_changes.insert_in_place(
            key.clone(),
            Change {
                old_value,
                new_value: Some(value.into()),
            },
        );
        Ok(())
    }

    pub(crate) fn remove(
        &mut self,
        key: &TrieKey,
        batch: Option<&mut DB::Batch>,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        trace!("Removing from KeyValueDB: {:?}", key);
        let old_value = self.db.remove(&key.into(), batch)?;
        self.changes_store.current_changes.insert_in_place(
            key.clone(),
            Change {
                old_value,
                new_value: None,
            },
        );
        Ok(())
    }

    pub(crate) fn write_batch(
        &mut self,
        batch: DB::Batch,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        trace!("Writing batch into KeyValueDB");
        Ok(self.db.write_batch(batch)?)
    }
}

impl<DB, ID> KeyValueDB<DB, ID>
where
    ID: Id,
    DB: BonsaiDatabase + BonsaiPersistentDatabase<ID>,
{
    pub(crate) fn create_snapshot(&mut self, id: ID) {
        if self.snap_counter % self.config.snapshot_interval == 0 {
            self.db.snapshot(id);
            self.snap_holder.insert(id);
            if let Some(max_saved_snapshots) = self.config.max_saved_snapshots {
                if self.snap_holder.len() > max_saved_snapshots {
                    self.snap_holder.pop_first();
                }
            }
        }
        self.snap_counter += 1;
    }

    pub(crate) fn get_transaction(
        &self,
        id: ID,
    ) -> Result<
        Option<DB::Transaction>,
        BonsaiStorageError<<DB::Transaction as BonsaiDatabase>::DatabaseError>,
    > {
        let Some(change_id) = self.snap_holder.range(..=id).last() else {
            return Ok(None);
        };
        let Some(mut txn) = self.db.transaction(*change_id) else {
            return Ok(None);
        };
        let Ok(snapshot_position) = self.changes_store.id_queue.binary_search(change_id) else {
            return Err(BonsaiStorageError::Transaction(format!(
                "id queue is missing {:?}",
                change_id
            )));
        };

        let mut batch = txn.create_batch();
        let iter = self
            .changes_store
            .id_queue
            .iter()
            .skip(snapshot_position)
            .take_while(|&&x| x <= id);
        for id in iter {
            let changes = ChangeBatch::deserialize(
                id,
                self.db
                    .get_by_prefix(&DatabaseKey::TrieLog(&id.to_bytes()))
                    .map_err(|_| {
                        BonsaiStorageError::Transaction(format!(
                            "database is missing trie logs for {:?}",
                            id
                        ))
                    })?,
            );
            for (key, change) in changes.0 {
                let key = DatabaseKey::from(&key);
                match (&change.old_value, &change.new_value) {
                    (Some(_), Some(new_value)) => {
                        txn.insert(&key, new_value, Some(&mut batch))?;
                    }
                    (Some(_), None) => {
                        txn.remove(&key, Some(&mut batch))?;
                    }
                    (None, Some(new_value)) => {
                        txn.insert(&key, new_value, Some(&mut batch))?;
                    }
                    (None, None) => unreachable!(),
                };
            }
        }
        txn.write_batch(batch)?;
        Ok(Some(txn))
    }

    pub(crate) fn merge(
        &mut self,
        transaction: KeyValueDB<DB::Transaction, ID>,
    ) -> Result<(), BonsaiStorageError<<DB as BonsaiPersistentDatabase<ID>>::DatabaseError>> {
        let Some(created_at) = transaction.created_at else {
            return Err(BonsaiStorageError::Merge(
                "Transaction has no created_at".to_string(),
            ));
        };
        let Some(last_recorded_change_id) = self.changes_store.id_queue.back() else {
            return Err(BonsaiStorageError::Merge(
                "No recorded change id".to_string(),
            ));
        };
        if &created_at >= last_recorded_change_id {
            self.changes_store.id_queue = transaction.changes_store.id_queue;
            self.db.merge(transaction.db)?;
        } else {
            return Err(BonsaiStorageError::Merge(format!(
                "Transaction created_at {:?} is lower than the last recorded id",
                created_at,
            )));
        }
        Ok(())
    }
}
