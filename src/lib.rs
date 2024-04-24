//! This crate provides a storage implementation based on the Bonsai Storage implemented by [Besu](https://hackmd.io/@kt2am/BktBblIL3).
//! It is a key/value storage that uses a Madara Merkle Trie to store the data.
//! This implementation can be used with any database that implements the `BonsaiDatabase` trait.
//!
//! Example usage with a RocksDB database:
//! ```ignore
//! # use bonsai_trie::{
//! #     databases::{RocksDB, create_rocks_db, RocksDBConfig},
//! #     BonsaiStorageError,
//! #     id::{BasicIdBuilder, BasicId},
//! #     BonsaiStorage, BonsaiStorageConfig, BonsaiTrieHash,
//! # };
//! # use starknet_types_core::felt::Felt;
//! # use starknet_types_core::hash::Pedersen;
//! # use bitvec::prelude::*;
//! let db = create_rocks_db("./rocksdb").unwrap();
//! let config = BonsaiStorageConfig::default();
//!
//! let identifier = vec![];
//! let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> = BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
//! let mut id_builder = BasicIdBuilder::new();
//!
//! let pair1 = (vec![1, 2, 1], Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap());
//! let bitvec_1 = BitVec::from_vec(pair1.0.clone());
//! bonsai_storage.insert(&identifier, &bitvec_1, &pair1.1).unwrap();
//!
//! let pair2 = (vec![1, 2, 2], Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap());
//! let bitvec = BitVec::from_vec(pair2.0.clone());
//! bonsai_storage.insert(&identifier, &bitvec, &pair2.1).unwrap();
//!
//! let id1 = id_builder.new_id();
//! bonsai_storage.commit(id1);
//!
//! let pair3 = (vec![1, 2, 2], Felt::from_hex("0x664D033c195fec3ce2568b62052e").unwrap());
//! let bitvec = BitVec::from_vec(pair3.0.clone());
//! bonsai_storage.insert(&identifier, &bitvec, &pair3.1).unwrap();
//!
//! let revert_to_id = id_builder.new_id();
//! bonsai_storage.commit(revert_to_id);
//!
//! bonsai_storage.remove(&identifier, &bitvec).unwrap();
//!
//! bonsai_storage.commit(id_builder.new_id());
//!
//! println!("root: {:#?}", bonsai_storage.root_hash(&identifier));
//! println!(
//!     "value: {:#?}",
//!     bonsai_storage.get(&identifier, &bitvec_1).unwrap()
//! );
//!
//! bonsai_storage.revert_to(revert_to_id).unwrap();
//!
//! println!("root: {:#?}", bonsai_storage.root_hash(&identifier));
//! println!("value: {:#?}", bonsai_storage.get(&identifier, &bitvec).unwrap());
//! std::thread::scope(|s| {
//!     s.spawn(|| {
//!         let bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
//!             .get_transactional_state(id1, bonsai_storage.get_config())
//!             .unwrap()
//!             .unwrap();
//!         let bitvec = BitVec::from_vec(pair1.0.clone());
//!         assert_eq!(bonsai_at_txn.get(&identifier, &bitvec).unwrap().unwrap(), pair1.1);
//!     });
//!
//!     s.spawn(|| {
//!         let bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
//!             .get_transactional_state(id1, bonsai_storage.get_config())
//!             .unwrap()
//!             .unwrap();
//!         let bitvec = BitVec::from_vec(pair1.0.clone());
//!         assert_eq!(bonsai_at_txn.get(&identifier, &bitvec).unwrap().unwrap(), pair1.1);
//!     });
//! });
//! bonsai_storage
//!     .get(&identifier, &BitVec::from_vec(vec![1, 2, 2]))
//!     .unwrap();
//! let pair2 = (
//!     vec![1, 2, 3],
//!     Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
//! );
//! bonsai_storage
//!     .insert(&identifier, &BitVec::from_vec(pair2.0.clone()), &pair2.1)
//!     .unwrap();
//! bonsai_storage.commit(id_builder.new_id()).unwrap();
//! ```
#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(not(feature = "std"))]
extern crate alloc;

use crate::trie::merkle_tree::{bytes_to_bitvec, MerkleTree};
#[cfg(not(feature = "std"))]
use alloc::{format, vec::Vec};
use bitvec::{order::Msb0, slice::BitSlice, vec::BitVec};
use changes::ChangeBatch;
use hashbrown::HashMap;
use key_value_db::KeyValueDB;
use starknet_types_core::{
    felt::Felt,
    hash::{Pedersen, StarkHash},
};

mod changes;
mod key_value_db;
mod trie;

mod bonsai_database;
/// All databases already implemented in this crate.
pub mod databases;
mod error;
/// Definition and basic implementation of an CommitID
pub mod id;

pub use bonsai_database::{BonsaiDatabase, BonsaiPersistentDatabase, DBError, DatabaseKey};
pub use error::BonsaiStorageError;
pub use trie::merkle_tree::{Membership, ProofNode};
use trie::{
    merkle_tree::{bitslice_to_bytes, InsertOrRemove, MerkleTrees},
    TrieKey, TrieKeyType,
};

/// Structure that contains the configuration for the BonsaiStorage.
/// A default implementation is provided with coherent values.
#[derive(Clone)]
pub struct BonsaiStorageConfig {
    /// Maximal number of trie logs saved.
    /// This corresponds to the number of latest commits that is saved in order to allow reverting or getting transactional state.
    /// Commits older than this limit are discarded and cannot be used.
    /// A value of None disables the limit and all commits since the trie creation are kept.
    /// Note that patch of changes between commits occupy space in the database.
    pub max_saved_trie_logs: Option<usize>,
    /// How many of the latest snapshots are saved, older ones are discarded.
    /// Higher values cause more database space usage, while lower values prevent the efficient reverting and creation of transactional states at older commits.
    pub max_saved_snapshots: Option<usize>,
    /// A database snapshot is created every `snapshot_interval` commits.
    /// Having more frequent snapshots occupies more disk space and has a slight performance impact on commits, but allows for more efficient transactional state creation.
    pub snapshot_interval: u64,
}

impl Default for BonsaiStorageConfig {
    fn default() -> Self {
        Self {
            max_saved_trie_logs: Some(500),
            max_saved_snapshots: Some(100),
            snapshot_interval: 5,
        }
    }
}

/// Structure used to represent a change in the trie for a specific value.
/// It contains the old value and the new value.
/// If the `old_value` is None, it means that the key was not present in the trie before the change.
/// If the `new_value` is None, it means that the key was removed from the trie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub old_value: Option<Felt>,
    pub new_value: Option<Felt>,
}

/// Structure that hold the trie and all the necessary information to work with it.
///
/// This structure is the main entry point to work with this crate.
pub struct BonsaiStorage<ChangeID, DB, H>
where
    DB: BonsaiDatabase,
    ChangeID: id::Id,
    H: StarkHash + Send + Sync,
{
    tries: MerkleTrees<H, DB, ChangeID>,
}

#[cfg(feature = "bench")]
impl<ChangeID, DB, H> Clone for BonsaiStorage<ChangeID, DB, H>
where
    DB: BonsaiDatabase + Clone,
    ChangeID: id::Id,
    H: StarkHash + Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            tries: self.tries.clone(),
        }
    }
}

/// Trie root hash type.
pub type BonsaiTrieHash = Felt;

impl<ChangeID, DB, H> BonsaiStorage<ChangeID, DB, H>
where
    DB: BonsaiDatabase,
    ChangeID: id::Id,
    H: StarkHash + Send + Sync,
{
    /// Create a new bonsai storage instance
    pub fn new(
        db: DB,
        config: BonsaiStorageConfig,
    ) -> Result<Self, BonsaiStorageError<DB::DatabaseError>> {
        let key_value_db = KeyValueDB::new(db, config.into(), None);
        Ok(Self {
            tries: MerkleTrees::new(key_value_db),
        })
    }

    pub fn new_from_transactional_state(
        db: DB,
        config: BonsaiStorageConfig,
        created_at: ChangeID,
        identifiers: Vec<Vec<u8>>,
    ) -> Result<Self, BonsaiStorageError<DB::DatabaseError>> {
        let key_value_db = KeyValueDB::new(db, config.into(), Some(created_at));
        let mut tries = MerkleTrees::<H, DB, ChangeID>::new(key_value_db);
        for identifier in identifiers {
            tries.init_tree(&identifier)?;
        }
        Ok(Self { tries })
    }

    /// Initialize a new trie with the given identifier.
    /// This function is useful when you want to create a new trie in the database without inserting any value.
    /// If the trie already exists, it will do nothing.
    /// When you insert a value in a trie, it will automatically create the trie if it doesn't exist.
    pub fn init_tree(
        &mut self,
        identifier: &[u8],
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        self.tries.init_tree(identifier)
    }

    /// Insert a new key/value in the trie, overwriting the previous value if it exists.
    /// If the value already exists it will overwrite it.
    ///
    /// Be careful to provide a key that does not collide with those already present in storage,
    /// as [RevertibleStorage] does not handle collisions automatically yet.
    ///
    /// > Note: changes will not be applied until the next `commit`
    pub fn insert(
        &mut self,
        identifier: &[u8],
        key: &BitSlice<u8, Msb0>,
        value: &Felt,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        self.tries.set(identifier, key, *value)?;
        Ok(())
    }

    /// Remove a key/value in the trie
    /// If the value doesn't exist it will do nothing
    pub fn remove(
        &mut self,
        identifier: &[u8],
        key: &BitSlice<u8, Msb0>,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        self.tries.set(identifier, key, Felt::ZERO)?;
        Ok(())
    }

    /// Get a value in the trie.
    pub fn get(
        &self,
        identifier: &[u8],
        key: &BitSlice<u8, Msb0>,
    ) -> Result<Option<Felt>, BonsaiStorageError<DB::DatabaseError>> {
        self.tries.get(identifier, key)
    }

    /// Checks if the key exists in the trie.
    pub fn contains(
        &self,
        identifier: &[u8],
        key: &BitSlice<u8, Msb0>,
    ) -> Result<bool, BonsaiStorageError<DB::DatabaseError>> {
        self.tries.contains(identifier, key)
    }

    /// Go to a specific commit ID.
    /// If insert/remove is called between the last `commit()` and a call to this function,
    /// the in-memory changes will be discarded.
    pub fn revert_to(
        &mut self,
        requested_id: ChangeID,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        let kv = self.tries.db_mut();

        // Clear current changes
        kv.changes_store.current_changes.0.clear();

        // If requested equals last recorded, do nothing
        if Some(&requested_id) == kv.changes_store.id_queue.back() {
            return Ok(());
        }

        // Make sure we are not trying to revert with an invalid id
        let Some(id_position) = kv
            .changes_store
            .id_queue
            .iter()
            .position(|id| *id == requested_id)
        else {
            return Err(BonsaiStorageError::GoTo(format!(
                "Requested id {:?} was removed or has not been recorded",
                requested_id
            )));
        };

        // Accumulate changes from requested to last recorded
        let mut full = Vec::new();
        for id in kv
            .changes_store
            .id_queue
            .iter()
            .skip(id_position)
            .rev()
            .take_while(|id| *id != &requested_id)
        {
            full.extend(
                ChangeBatch::deserialize(
                    id,
                    kv.db.get_by_prefix(&DatabaseKey::TrieLog(&id.to_bytes()))?,
                )
                .0,
            );
        }

        // Revert changes
        let mut batch = kv.db.create_batch();
        for (key, change) in full.iter().rev() {
            let key = DatabaseKey::from(key);
            match (&change.old_value, &change.new_value) {
                (Some(old_value), Some(_)) => {
                    kv.db.insert(&key, old_value, Some(&mut batch))?;
                }
                (Some(old_value), None) => {
                    kv.db.insert(&key, old_value, Some(&mut batch))?;
                }
                (None, Some(_)) => {
                    kv.db.remove(&key, Some(&mut batch))?;
                }
                (None, None) => unreachable!(),
            };
        }

        // Truncate trie logs at the requested id
        let mut truncated = kv.changes_store.id_queue.split_off(id_position);
        if let Some(current) = truncated.pop_front() {
            kv.changes_store.id_queue.push_back(current);
        }
        for id in truncated.iter() {
            kv.db
                .remove_by_prefix(&DatabaseKey::TrieLog(&id.to_bytes()))?;
        }

        // Write revert changes and trie logs truncation
        kv.db.write_batch(batch)?;
        self.tries.reset_to_last_commit()?;
        Ok(())
    }

    /// Get all changes applied at a certain commit ID.
    #[allow(clippy::type_complexity)]
    pub fn get_changes(
        &self,
        id: ChangeID,
    ) -> Result<HashMap<BitVec<u8, Msb0>, Change>, BonsaiStorageError<DB::DatabaseError>> {
        self.tries.db_ref().get_changes(id)
    }

    #[cfg(test)]
    pub fn dump_database(&self) {
        self.tries.db_ref().db.dump_database();
    }

    /// Get trie root hash at the latest commit
    pub fn root_hash(
        &self,
        identifier: &[u8],
    ) -> Result<BonsaiTrieHash, BonsaiStorageError<DB::DatabaseError>> {
        self.tries.root_hash(identifier)
    }

    /// This function must be used with transactional state only.
    /// Similar to `commit` but does not create any snapshot.
    // TODO: make it so this is ONLY acessible from transactional state (type seperation)
    pub fn transactional_commit(
        &mut self,
        id: ChangeID,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        self.tries.commit()?;
        self.tries.db_mut().commit(id)?;
        Ok(())
    }

    /// Generates a merkle-proof for a given `key`.
    ///
    /// Returns vector of [`TrieNode`] which form a chain from the root to the key,
    /// if it exists, or down to the node which proves that the key does not exist.
    ///
    /// The nodes are returned in order, root first.
    ///
    /// Verification is performed by confirming that:
    ///   1. the chain follows the path of `key`, and
    ///   2. the hashes are correct, and
    ///   3. the root hash matches the known root
    pub fn get_proof(
        &self,
        identifier: &[u8],
        key: &BitSlice<u8, Msb0>,
    ) -> Result<Vec<ProofNode>, BonsaiStorageError<DB::DatabaseError>> {
        self.tries.get_proof(identifier, key)
    }

    /// Get all the keys in a specific trie.
    pub fn get_keys(
        &self,
        identifier: &[u8],
    ) -> Result<Vec<Vec<u8>>, BonsaiStorageError<DB::DatabaseError>> {
        self.tries.get_keys(identifier)
    }

    /// Get all the key-value pairs in a specific trie.
    #[allow(clippy::type_complexity)]
    pub fn get_key_value_pairs(
        &self,
        identifier: &[u8],
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, BonsaiStorageError<DB::DatabaseError>> {
        self.tries.get_key_value_pairs(identifier)
    }

    /// Get the id from the latest commit, or `None` if no commit has taken place yet.
    pub fn get_latest_id(&self) -> Option<ChangeID> {
        self.tries.db_ref().get_latest_id()
    }

    /// Verifies a merkle-proof for a given `key` and `value`.
    pub fn verify_proof(
        root: Felt,
        key: &BitSlice<u8, Msb0>,
        value: Felt,
        proofs: &[ProofNode],
    ) -> Option<Membership> {
        MerkleTree::<Pedersen>::verify_proof(root, key, value, proofs)
    }
}

impl<ChangeID, DB, H> BonsaiStorage<ChangeID, DB, H>
where
    DB: BonsaiDatabase + BonsaiPersistentDatabase<ChangeID>,
    ChangeID: id::Id,
    H: StarkHash + Send + Sync,
{
    /// Update trie and database using all changes since the last commit.
    pub fn commit(
        &mut self,
        id: ChangeID,
    ) -> Result<(), BonsaiStorageError<<DB as BonsaiDatabase>::DatabaseError>> {
        self.tries.commit()?;
        self.tries.db_mut().commit(id)?;
        self.tries.db_mut().create_snapshot(id);
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    /// Get a transactional state of the trie at a specific commit ID.
    ///
    /// Transactional state allow you to fetch a point-in-time state of the trie. You can
    /// apply changes to this state and merge it back into the main trie.
    ///
    /// > Note that a new transactional state will be created based on the nearest snapshot.
    pub fn get_transactional_state(
        &self,
        change_id: ChangeID,
        config: BonsaiStorageConfig,
    ) -> Result<
        Option<BonsaiStorage<ChangeID, DB::Transaction, H>>,
        BonsaiStorageError<<DB::Transaction as BonsaiDatabase>::DatabaseError>,
    > {
        if let Some(transaction) = self.tries.db_ref().get_transaction(change_id)? {
            Ok(Some(BonsaiStorage::new_from_transactional_state(
                transaction,
                config,
                change_id,
                self.tries.get_identifiers(),
            )?))
        } else {
            Ok(None)
        }
    }

    /// Get a copy of the config that can be used to create a transactional state or a new bonsai storage.
    pub fn get_config(&self) -> BonsaiStorageConfig {
        self.tries.db_ref().get_config().into()
    }

    /// Get a copy of all trie identifiers used by the db.
    pub fn get_identifiers(&self) -> Vec<Vec<u8>> {
        self.tries.get_identifiers()
    }

    /// Merge a transactional state into the main trie.
    pub fn merge(
        &mut self,
        transactional_bonsai_storage: BonsaiStorage<ChangeID, DB::Transaction, H>,
    ) -> Result<(), BonsaiStorageError<<DB as BonsaiPersistentDatabase<ChangeID>>::DatabaseError>>
    where
        <DB as BonsaiDatabase>::DatabaseError: core::fmt::Debug,
    {
        // memorize changes
        let MerkleTrees { db, trees, .. } = transactional_bonsai_storage.tries;

        self.tries.db_mut().merge(db)?;

        // apply changes
        for (identifier, tree) in trees {
            for (k, op) in tree.cache_leaf_modified() {
                match op {
                    crate::trie::merkle_tree::InsertOrRemove::Insert(v) => {
                        self.insert(&identifier, &bytes_to_bitvec(k), v)
                            .map_err(|e| {
                                BonsaiStorageError::Merge(format!(
                                    "While merging insert({:?} {}) faced error: {:?}",
                                    k, v, e
                                ))
                            })?;
                    }
                    crate::trie::merkle_tree::InsertOrRemove::Remove => {
                        self.remove(&identifier, &bytes_to_bitvec(k)).map_err(|e| {
                            BonsaiStorageError::Merge(format!(
                                "While merging remove({:?}) faced error: {:?}",
                                k, e
                            ))
                        })?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// An alternative to [BonsaiStorage] that does not store data in a trie.
///
/// Use this to store data that does not need to be verified but still has to be revertible.
pub struct RevertibleStorage<ChangeID, DB>
where
    DB: BonsaiDatabase,
    ChangeID: id::Id,
{
    db: KeyValueDB<DB, ChangeID>,
    cache_storage_modified: HashMap<TrieKey, InsertOrRemove<Vec<u8>>>,
}

impl<ChangeID, DB> RevertibleStorage<ChangeID, DB>
where
    DB: BonsaiDatabase,
    ChangeID: id::Id,
{
    /// Create a new revertible storage instance
    pub fn new(
        db: DB,
        config: BonsaiStorageConfig,
    ) -> Result<Self, BonsaiStorageError<DB::DatabaseError>> {
        let kvdb = KeyValueDB::new(db, config.into(), None);
        Ok(Self {
            db: kvdb,
            cache_storage_modified: HashMap::new(),
        })
    }

    pub fn new_from_transactional_state(
        db: DB,
        config: BonsaiStorageConfig,
        created_at: ChangeID,
    ) -> Result<Self, BonsaiStorageError<DB::DatabaseError>> {
        let kvdb = KeyValueDB::new(db, config.into(), Some(created_at));
        Ok(Self {
            db: kvdb,
            cache_storage_modified: HashMap::new(),
        })
    }

    /// Insert a new key/value in the storage, overwriting the previous value if it exists.
    /// If the value already exists it will overwrite it.
    ///
    /// Be careful to provide a key that does not collide with those already present in storage,
    /// as [RevertibleStorage] does not handle collisions automatically yet.
    ///
    /// > Note: changes will not be applied until the next `commit`
    pub fn insert(&mut self, key: &BitSlice<u8, Msb0>, value: &[u8]) {
        let key = bitslice_to_bytes(key);

        self.cache_storage_modified.insert(
            TrieKey::new(&[], TrieKeyType::Flat, &key),
            InsertOrRemove::Insert(value.to_vec()),
        );
    }

    /// Remove a key/value in the storage
    /// If the value doesn't exist it will do nothing
    ///
    /// > Note: changes will not be applied until the next `commit`
    pub fn remove(&mut self, key: &BitSlice<u8, Msb0>) {
        let key = bitslice_to_bytes(key);

        self.cache_storage_modified.insert(
            TrieKey::new(&[], TrieKeyType::Flat, &key),
            InsertOrRemove::Remove,
        );
    }

    /// Get a value in the storage.
    pub fn get(
        &self,
        key: &BitSlice<u8, Msb0>,
    ) -> Result<Option<Vec<u8>>, BonsaiStorageError<DB::DatabaseError>> {
        let key = bitslice_to_bytes(key);
        let key = TrieKey::new(&[], TrieKeyType::Flat, &key);

        match self.db.get(&key)? {
            Some(value) => Ok(Some(value)),
            None => Ok(None),
        }
    }

    /// Checks if the key exists in the storage.
    pub fn contains(
        &self,
        key: &BitSlice<u8, Msb0>,
    ) -> Result<bool, BonsaiStorageError<DB::DatabaseError>> {
        let key = bitslice_to_bytes(key);
        self.db
            .contains(&TrieKey::new(&[], TrieKeyType::Flat, &key))
    }

    /// Go to a specific commit ID.
    /// If insert/remove is called between the last `commit()` and a call to this function,
    /// the in-memory changes will be discarded.
    pub fn revert_to(
        &mut self,
        requested_id: ChangeID,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        let kv = &mut self.db;

        // Clear current changes
        kv.changes_store.current_changes.0.clear();

        // If requested equals last recorded, do nothing
        if Some(&requested_id) == kv.changes_store.id_queue.back() {
            return Ok(());
        }

        // Make sure we are not trying to revert with an invalid id
        let Some(id_position) = kv
            .changes_store
            .id_queue
            .iter()
            // TODO: this should be repleaceable by binary search since each new id
            // must be greater than the last
            .position(|id| *id == requested_id)
        else {
            return Err(BonsaiStorageError::GoTo(format!(
                "Requested id {:?} was removed or has not been recorded",
                requested_id
            )));
        };

        // Accumulate changes from requested to last recorded
        let mut full = Vec::new();
        for id in kv
            .changes_store
            .id_queue
            .iter()
            .skip(id_position)
            .rev()
            .take_while(|id| *id != &requested_id)
        {
            full.extend(
                ChangeBatch::deserialize(
                    id,
                    kv.db.get_by_prefix(&DatabaseKey::TrieLog(&id.to_bytes()))?,
                )
                .0,
            );
        }

        let mut batch = kv.db.create_batch();
        for (key, change) in full.iter().rev() {
            let key = DatabaseKey::from(key);
            match (&change.old_value, &change.new_value) {
                (Some(old_value), Some(_)) => {
                    kv.db.insert(&key, old_value, Some(&mut batch))?;
                }
                (Some(old_value), None) => {
                    kv.db.insert(&key, old_value, Some(&mut batch))?;
                }
                (None, Some(_)) => {
                    kv.db.remove(&key, Some(&mut batch))?;
                }
                (None, None) => unreachable!(),
            };
        }

        // Truncate trie logs at the requested id
        let mut truncated = kv.changes_store.id_queue.split_off(id_position);
        if let Some(current) = truncated.pop_front() {
            kv.changes_store.id_queue.push_back(current);
        }
        for id in truncated.iter() {
            kv.db
                .remove_by_prefix(&DatabaseKey::TrieLog(&id.to_bytes()))?;
        }

        // Write revert changes
        kv.db.write_batch(batch)?;
        Ok(())
    }

    /// Get all changes applied at a certain commit ID.
    #[allow(clippy::type_complexity)]
    pub fn get_changes(
        &self,
        id: ChangeID,
    ) -> Result<HashMap<BitVec<u8, Msb0>, Change>, BonsaiStorageError<DB::DatabaseError>> {
        self.db.get_changes(id)
    }

    /// This function must be used with transactional state only.
    /// Similar to `commit` but does not create any snapshot.
    // TODO: make it so this is ONLY acessible from transactional state (type seperation)
    pub fn transactional_commit(
        &mut self,
        id: ChangeID,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        let mut batch = self.db.create_batch();
        for (key, value) in self.cache_storage_modified.iter() {
            match value {
                InsertOrRemove::Insert(value) => self.db.insert(key, value, Some(&mut batch))?,
                InsertOrRemove::Remove => self.db.remove(key, Some(&mut batch))?,
            }
        }
        self.cache_storage_modified = HashMap::new();

        self.db.write_batch(batch)?;
        self.db.commit(id)?;
        Ok(())
    }
}

impl<ChangeID, DB> RevertibleStorage<ChangeID, DB>
where
    DB: BonsaiDatabase + BonsaiPersistentDatabase<ChangeID>,
    ChangeID: id::Id,
{
    pub fn commit(
        &mut self,
        id: ChangeID,
    ) -> Result<(), BonsaiStorageError<<DB as BonsaiDatabase>::DatabaseError>> {
        let mut batch = self.db.create_batch();
        for (key, value) in self.cache_storage_modified.iter() {
            match value {
                InsertOrRemove::Insert(value) => self.db.insert(key, value, Some(&mut batch))?,
                InsertOrRemove::Remove => self.db.remove(key, Some(&mut batch))?,
            }
        }
        self.cache_storage_modified = HashMap::new();

        self.db.write_batch(batch)?;
        self.db.commit(id)?;
        self.db.create_snapshot(id);
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    /// Get a transactional state of the storage at a specific commit ID.
    ///
    /// Transactional state allow you to fetch a point-in-time state of the storage. You can
    /// apply changes to this state and merge it back into the main storage.
    ///
    /// > Note that a new transactional state will be created based on the nearest snapshot.
    pub fn get_transactional_state(
        &self,
        change_id: ChangeID,
        config: BonsaiStorageConfig,
    ) -> Result<
        Option<RevertibleStorage<ChangeID, DB::Transaction>>,
        BonsaiStorageError<<DB::Transaction as BonsaiDatabase>::DatabaseError>,
    > {
        if let Some(transaction) = self.db.get_transaction(change_id)? {
            Ok(Some(RevertibleStorage::new_from_transactional_state(
                transaction,
                config,
                change_id,
            )?))
        } else {
            Ok(None)
        }
    }

    /// Returns the config used to set up this [RevertibleStorage]
    pub fn get_config(&self) -> BonsaiStorageConfig {
        self.db.get_config().into()
    }

    /// Merge a transactional state into the main storage.
    pub fn merge(
        &mut self,
        transactional_revertible_storage: RevertibleStorage<ChangeID, DB::Transaction>,
    ) -> Result<(), BonsaiStorageError<<DB as BonsaiPersistentDatabase<ChangeID>>::DatabaseError>>
    {
        self.db.merge(transactional_revertible_storage.db)
    }
}

#[cfg(test)]
#[cfg(all(test, feature = "std"))]
mod tests {
    use bitvec::{order::Msb0, vec::BitVec, view::BitView};

    use crate::{
        databases::{create_rocks_db, RocksDB, RocksDBConfig},
        id::BasicId,
        BonsaiStorageConfig, Felt, RevertibleStorage,
    };

    #[test_log::test]
    fn test_revertible_storage() {
        log::info!("Creating revertible storage...");
        let tempdir = tempfile::tempdir().unwrap();
        let rocksdb = create_rocks_db(tempdir).unwrap();
        let db = RocksDB::new(&rocksdb, RocksDBConfig::default());
        let mut revertible = RevertibleStorage::new(db, BonsaiStorageConfig::default()).unwrap();

        let data = vec![
            (
                key("0x0000000000000000000000000000000000000000000000000000000000000005"),
                value("0x0000000000000000000000000000000000000000000000000000000000000065"),
            ),
            (
                key("0x00cfc2e2866fd08bfb4ac73b70e0c136e326ae18fc797a2c090c8811c695577e"),
                value("0x05f1dd5a5aef88e0498eeca4e7b2ea0fa7110608c11531278742f0b5499af4b3"),
            ),
            (
                key("0x05aee31408163292105d875070f98cb48275b8c87e80380b78d30647e05854d5"),
                value("0x00000000000000000000000000000000000000000000000000000000000007c7"),
            ),
            (
                key("0x05fac6815fddf6af1ca5e592359862ede14f171e1544fd9e792288164097c35d"),
                value("0x00299e2f4b5a873e95e65eb03d31e532ea2cde43b498b50cd3161145db5542a5"),
            ),
            (
                key("0x05fac6815fddf6af1ca5e592359862ede14f171e1544fd9e792288164097c35e"),
                value("0x03d6897cf23da3bf4fd35cc7a43ccaf7c5eaf8f7c5b9031ac9b09a929204175f"),
            ),
        ];

        log::info!("Testing k-v insertion...");
        for (key, value) in data.iter() {
            revertible.insert(&key, &value);
        }

        assert!(revertible.commit(BasicId::new(0)).is_ok());

        log::info!("Testing k-v retrieval...");
        for (key, value) in data.iter() {
            let result = revertible.get(&key);
            assert!(result.is_ok());

            let result = result.unwrap().unwrap();
            assert_eq!(result, *value);
        }
    }

    #[test]
    fn test_revertible_storage_transactional_state() {
        let tempdir = tempfile::tempdir().unwrap();
        let rocksdb = create_rocks_db(tempdir).unwrap();
        let db = RocksDB::new(&rocksdb, RocksDBConfig::default());
        let mut revertible = RevertibleStorage::new(db, BonsaiStorageConfig::default()).unwrap();

        revertible.insert(&key(&"0x01"), &value(&"0x01"));
        assert!(revertible.commit(BasicId::new(0)).is_ok());

        revertible.insert(&key(&"0x02"), &value(&"0x02"));
        assert!(revertible.commit(BasicId::new(1)).is_ok());

        revertible.insert(&key(&"0x03"), &value(&"0x03"));
        assert!(revertible.commit(BasicId::new(2)).is_ok());

        // transactional state for commit id 0 should ONLY contain 0x01 key
        let state_0 = match revertible
            .get_transactional_state(BasicId::new(0), BonsaiStorageConfig::default())
        {
            Ok(Some(state)) => state,
            _ => panic!("Failed to get transactional state for commit id 0"),
        };
        assert_eq!(state_0.get(&key(&"0x01")).unwrap(), Some(value("0x01")));
        assert_eq!(state_0.get(&key(&"0x02")).unwrap(), None);
        assert_eq!(state_0.get(&key(&"0x03")).unwrap(), None);

        // transactional state for commit id 1 should contain 0x01 and 0x02 keys
        let state_1 = match revertible
            .get_transactional_state(BasicId::new(1), BonsaiStorageConfig::default())
        {
            Ok(Some(state)) => state,
            _ => panic!("Failed to get transactional state for commit id 0"),
        };
        assert_eq!(state_1.get(&key(&"0x01")).unwrap(), Some(value("0x01")));
        assert_eq!(state_1.get(&key(&"0x02")).unwrap(), Some(value("0x02")));
        assert_eq!(state_1.get(&key(&"0x03")).unwrap(), None);

        // transactional state for commit id 2 should contain 0x01, 0x02 and 0x03 keys
        let state_2 = match revertible
            .get_transactional_state(BasicId::new(2), BonsaiStorageConfig::default())
        {
            Ok(Some(state)) => state,
            _ => panic!("Failed to get transactional state for commit id 0"),
        };
        assert_eq!(state_2.get(&key(&"0x01")).unwrap(), Some(value("0x01")));
        assert_eq!(state_2.get(&key(&"0x02")).unwrap(), Some(value("0x02")));
        assert_eq!(state_2.get(&key(&"0x03")).unwrap(), Some(value("0x03")));
    }

    #[test]
    fn test_revertible_storage_revert_to() {
        let tempdir = tempfile::tempdir().unwrap();
        let rocksdb = create_rocks_db(tempdir).unwrap();
        let db = RocksDB::new(&rocksdb, RocksDBConfig::default());
        let mut revertible = RevertibleStorage::new(db, BonsaiStorageConfig::default()).unwrap();

        revertible.insert(&key(&"0x01"), &value(&"0x01"));
        assert!(revertible.commit(BasicId::new(0)).is_ok());

        revertible.insert(&key(&"0x02"), &value(&"0x02"));
        assert!(revertible.commit(BasicId::new(1)).is_ok());

        revertible.insert(&key(&"0x03"), &value(&"0x03"));
        assert!(revertible.commit(BasicId::new(2)).is_ok());

        assert!(revertible.revert_to(BasicId::new(0)).is_ok());

        // only storage at key '0x01' should be accessible after revert
        assert_eq!(revertible.get(&key(&"0x01")).unwrap(), Some(value("0x01")));
        assert_eq!(revertible.get(&key(&"0x02")).unwrap(), None);
        assert_eq!(revertible.get(&key(&"0x03")).unwrap(), None);
    }

    fn key(hex: &str) -> BitVec<u8, Msb0> {
        Felt::from_hex(hex).unwrap().to_bytes_be().view_bits()[5..].to_owned()
    }

    fn value(hex: &str) -> Vec<u8> {
        Felt::from_hex(hex).unwrap().to_bytes_be().to_vec()
    }
}
