use crate::{bytes_to_bitvec, format, BitVec, ByteVec, Change as ExternChange};
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
#[derive(Debug)]
pub struct KeyValueDB<DB: BonsaiDatabase, ID: Id> {
    pub(crate) db: DB,
    pub(crate) changes_store: ChangeStore,
    pub(crate) config: KeyValueDBConfig,
    pub(crate) _created_at: Option<ID>,
}

#[derive(Clone, Debug)]
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
        let changes_store = ChangeStore::new();
        Self {
            db: underline_db,
            changes_store,
            config,
            _created_at: created_at,
        }
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn get_changes(
        &self,
        id: ID,
    ) -> Result<HashMap<BitVec, ExternChange>, BonsaiStorageError<DB::DatabaseError>> {
        let mut leaf_changes = HashMap::new();
        let changes = ChangeBatch::deserialize(
            &id,
            self.db
                .get_by_prefix(&DatabaseKey::TrieLog(&id.to_bytes()))?,
        );
        for (k, v) in changes.0 {
            if let TrieKey::Flat(k) = k {
                // Note on safety of expect():
                // We are sure that the values are valid Felt because they can be saved only by our crate
                let old_value = v.old_value.map(|x| {
                    Felt::decode(&mut x.as_ref()).expect(
                        "We saved this Felt ('old_value') so we should be able to decode it",
                    )
                });
                let new_value = v.new_value.map(|x| {
                    Felt::decode(&mut x.as_ref()).expect(
                        "We saved this Felt ('new_value') so we should be able to decode it",
                    )
                });
                leaf_changes.insert(
                    bytes_to_bitvec(&k),
                    ExternChange {
                        old_value,
                        new_value,
                    },
                );
            }
        }
        Ok(leaf_changes)
    }

    pub(crate) fn commit(&mut self, id: ID) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        // Insert flat db changes
        let mut batch = self.db.create_batch();
        let current_changes = core::mem::take(&mut self.changes_store.current_changes);
        log::debug!("Committing id {id:?}");

        if self.config.max_saved_trie_logs != Some(0) {
            // optim when trie logs are disabled.
            for (key, change) in current_changes.serialize(&id).iter() {
                self.db
                    .insert(&DatabaseKey::TrieLog(key), change, Some(&mut batch))?;
            }
            self.db.write_batch(batch)?;

            if let Some(id) = self
                .config
                .max_saved_trie_logs
                .and_then(|max_saved_trie_logs| id.as_u64().checked_sub(max_saved_trie_logs as _))
            {
                log::debug!("Remove by prefix {id:?}");
                self.db
                    .remove_by_prefix(&DatabaseKey::TrieLog(&ID::from_u64(id).to_bytes()))?;
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
        _key: &TrieKey,
        _id: ID,
    ) -> Result<Option<ByteVec>, BonsaiStorageError<DB::DatabaseError>> {
        todo!()
    }

    pub(crate) fn get_latest_id(&self) -> Option<ID> {
        todo!()
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
        if id.as_u64() % self.config.snapshot_interval == 0 {
            self.db.snapshot(id);
        }
    }

    pub(crate) fn get_transaction(
        &self,
        id: ID,
    ) -> Result<
        Option<DB::Transaction<'_>>,
        BonsaiStorageError<<DB::Transaction<'_> as BonsaiDatabase>::DatabaseError>,
    > {
        log::debug!("get_transaction {id:?}");
        let Some((snap_id, mut txn)) = self.db.transaction(id) else {
            return Ok(None);
        };
        log::debug!("get_transaction {snap_id:?} {id:?}");

        let mut batch = txn.create_batch();
        for cur_id in snap_id.as_u64()..id.as_u64() {
            let cur_id = ID::from_u64(cur_id);
            let changes = ChangeBatch::deserialize(
                &cur_id,
                self.db
                    .get_by_prefix(&DatabaseKey::TrieLog(&cur_id.to_bytes()))
                    .map_err(|_| {
                        BonsaiStorageError::Transaction(format!(
                            "database is missing trie logs for {:?}",
                            cur_id
                        ))
                    })?,
            );
            // Apply backwards
            for (key, change) in changes.0 {
                let key = DatabaseKey::from(&key);
                match (&change.old_value, &change.new_value) {
                    (Some(old_value), Some(_)) => {
                        txn.insert(&key, old_value, Some(&mut batch))?;
                    }
                    (Some(old_value), None) => {
                        txn.insert(&key, old_value, Some(&mut batch))?;
                    }
                    (None, Some(_)) => {
                        txn.remove(&key, Some(&mut batch))?;
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
        _transaction: KeyValueDB<DB::Transaction<'_>, ID>,
    ) -> Result<(), BonsaiStorageError<<DB as BonsaiPersistentDatabase<ID>>::DatabaseError>> {
        todo!("unused yet")
    }
}
