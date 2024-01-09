use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::{
    bonsai_database::{BonsaiDatabase, BonsaiPersistentDatabase, KeyType},
    changes::{Change, ChangeBatch, ChangeStore},
    id::Id,
    trie::TrieKeyType,
    BonsaiStorageConfig, BonsaiStorageError,
};

/// Crate Trie <= KeyValueDB => BonsaiDatabase
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
    BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
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

    pub(crate) fn commit(&mut self, id: ID) -> Result<(), BonsaiStorageError> {
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
                .insert(&KeyType::TrieLog(key), change, Some(&mut batch))?;
        }
        self.db.write_batch(batch)?;

        if let Some(max_saved_trie_logs) = self.config.max_saved_trie_logs {
            while self.changes_store.id_queue.len() > max_saved_trie_logs {
                // verified by previous conditional statement
                let id = self.changes_store.id_queue.pop_front().unwrap().serialize();
                self.db.remove_by_prefix(&KeyType::TrieLog(&id))?;
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

    pub(crate) fn get(&self, key: &TrieKeyType) -> Result<Option<Vec<u8>>, BonsaiStorageError> {
        Ok(self.db.get(&key.into())?)
    }

    pub(crate) fn contains(&self, key: &TrieKeyType) -> Result<bool, BonsaiStorageError> {
        Ok(self.db.contains(&key.into())?)
    }

    pub(crate) fn insert(
        &mut self,
        key: &TrieKeyType,
        value: &[u8],
        batch: Option<&mut DB::Batch>,
    ) -> Result<(), BonsaiStorageError> {
        let old_value = self.db.insert(&key.into(), value, batch)?;
        self.changes_store.current_changes.insert_in_place(
            key.into(),
            Change {
                old_value,
                new_value: Some(value.to_vec()),
            },
        );
        Ok(())
    }

    pub(crate) fn remove(
        &mut self,
        key: &TrieKeyType,
        batch: Option<&mut DB::Batch>,
    ) -> Result<(), BonsaiStorageError> {
        let old_value = self.db.remove(&key.into(), batch)?;
        self.changes_store.current_changes.insert_in_place(
            key.into(),
            Change {
                old_value,
                new_value: None,
            },
        );
        Ok(())
    }

    pub(crate) fn write_batch(&mut self, batch: DB::Batch) -> Result<(), BonsaiStorageError> {
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
    ) -> Result<Option<DB::Transaction>, BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB::Transaction as BonsaiDatabase>::DatabaseError>,
    {
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
                    .get_by_prefix(&KeyType::TrieLog(id.serialize().as_ref()))
                    .map_err(|_| {
                        BonsaiStorageError::Transaction(format!(
                            "database is missing trie logs for {:?}",
                            id
                        ))
                    })?,
            );
            for (key, change) in changes.0 {
                let key = KeyType::from(&key);
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
    ) -> Result<(), BonsaiStorageError>
    where
        BonsaiStorageError:
            core::convert::From<<DB as BonsaiPersistentDatabase<ID>>::DatabaseError>,
    {
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
