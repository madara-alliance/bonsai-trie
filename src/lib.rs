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
//! let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> = BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
//! let mut id_builder = BasicIdBuilder::new();
//!
//! let pair1 = (vec![1, 2, 1], Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap());
//! let bitvec_1 = BitVec::from_vec(pair1.0.clone());
//! bonsai_storage.insert(&bitvec_1, &pair1.1).unwrap();
//!
//! let pair2 = (vec![1, 2, 2], Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap());
//! let bitvec = BitVec::from_vec(pair2.0.clone());
//! bonsai_storage.insert(&bitvec, &pair2.1).unwrap();
//!
//! let id1 = id_builder.new_id();
//! bonsai_storage.commit(id1);
//!
//! let pair3 = (vec![1, 2, 2], Felt::from_hex("0x664D033c195fec3ce2568b62052e").unwrap());
//! let bitvec = BitVec::from_vec(pair3.0.clone());
//! bonsai_storage.insert(&bitvec, &pair3.1).unwrap();
//!
//! let revert_to_id = id_builder.new_id();
//! bonsai_storage.commit(revert_to_id);
//!
//! bonsai_storage.remove(&bitvec).unwrap();
//!
//! bonsai_storage.commit(id_builder.new_id());
//!
//! println!("root: {:#?}", bonsai_storage.root_hash());
//! println!(
//!     "value: {:#?}",
//!     bonsai_storage.get(&bitvec_1).unwrap()
//! );
//!
//! bonsai_storage.revert_to(revert_to_id).unwrap();
//!
//! println!("root: {:#?}", bonsai_storage.root_hash());
//! println!("value: {:#?}", bonsai_storage.get(&bitvec).unwrap());
//! std::thread::scope(|s| {
//!     s.spawn(|| {
//!         let bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
//!             .get_transactional_state(id1, bonsai_storage.get_config())
//!             .unwrap()
//!             .unwrap();
//!         let bitvec = BitVec::from_vec(pair1.0.clone());
//!         assert_eq!(bonsai_at_txn.get(&bitvec).unwrap().unwrap(), pair1.1);
//!     });
//!
//!     s.spawn(|| {
//!         let bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
//!             .get_transactional_state(id1, bonsai_storage.get_config())
//!             .unwrap()
//!             .unwrap();
//!         let bitvec = BitVec::from_vec(pair1.0.clone());
//!         assert_eq!(bonsai_at_txn.get(&bitvec).unwrap().unwrap(), pair1.1);
//!     });
//! });
//! bonsai_storage
//!     .get(&BitVec::from_vec(vec![1, 2, 2]))
//!     .unwrap();
//! let pair2 = (
//!     vec![1, 2, 3],
//!     Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
//! );
//! bonsai_storage
//!     .insert(&BitVec::from_vec(pair2.0.clone()), &pair2.1)
//!     .unwrap();
//! bonsai_storage.commit(id_builder.new_id()).unwrap();
//! ```
#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(not(feature = "std"))]
extern crate alloc;

use crate::trie::merkle_tree::MerkleTree;
#[cfg(not(feature = "std"))]
use alloc::{format, vec::Vec};
use bitvec::{order::Msb0, slice::BitSlice, vec::BitVec};
use bonsai_database::{BonsaiPersistentDatabase, DatabaseKey};
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

pub use bonsai_database::{BonsaiDatabase, DBError, BonsaiPersistentDatabase, DatabaseKey};
pub use error::BonsaiStorageError;
pub use trie::merkle_tree::{Membership, ProofNode};

#[cfg(test)]
mod tests;

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
    H: StarkHash,
{
    trie: MerkleTree<H, DB, ChangeID>,
}

/// Trie root hash type.
pub type BonsaiTrieHash = Felt;

impl<ChangeID, DB, H> BonsaiStorage<ChangeID, DB, H>
where
    DB: BonsaiDatabase,
    ChangeID: id::Id,
    H: StarkHash,
{
    /// Create a new bonsai storage instance
    pub fn new(
        db: DB,
        config: BonsaiStorageConfig,
    ) -> Result<Self, BonsaiStorageError<DB::DatabaseError>> {
        let key_value_db = KeyValueDB::new(db, config.into(), None);
        Ok(Self {
            trie: MerkleTree::new(key_value_db)?,
        })
    }

    pub fn new_from_transactional_state(
        db: DB,
        config: BonsaiStorageConfig,
        created_at: ChangeID,
    ) -> Result<Self, BonsaiStorageError<DB::DatabaseError>> {
        let key_value_db = KeyValueDB::new(db, config.into(), Some(created_at));
        Ok(Self {
            trie: MerkleTree::new(key_value_db)?,
        })
    }

    /// Insert a new key/value in the trie, overwriting the previous value if it exists.
    /// If the value already exists it will overwrite it.
    pub fn insert(
        &mut self,
        key: &BitSlice<u8, Msb0>,
        value: &Felt,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        self.trie.set(key, *value)?;
        Ok(())
    }

    /// Remove a key/value in the trie
    /// If the value doesn't exist it will do nothing
    pub fn remove(
        &mut self,
        key: &BitSlice<u8, Msb0>,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        self.trie.set(key, Felt::ZERO)?;
        Ok(())
    }

    /// Get a value in the trie.
    pub fn get(
        &self,
        key: &BitSlice<u8, Msb0>,
    ) -> Result<Option<Felt>, BonsaiStorageError<DB::DatabaseError>> {
        self.trie.get(key)
    }

    /// Checks if the key exists in the trie.
    pub fn contains(
        &self,
        key: &BitSlice<u8, Msb0>,
    ) -> Result<bool, BonsaiStorageError<DB::DatabaseError>> {
        self.trie.contains(key)
    }

    /// Go to a specific commit ID.
    /// If insert/remove is called between the last `commit()` and a call to this function,
    /// the in-memory changes will be discarded.
    pub fn revert_to(
        &mut self,
        requested_id: ChangeID,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        let kv = self.trie.db_mut();

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
        for id in kv.changes_store.id_queue.iter().skip(id_position).rev() {
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
        self.trie.reset_to_last_commit()?;
        Ok(())
    }

    /// Get all changes applied at a certain commit ID.
    #[allow(clippy::type_complexity)]
    pub fn get_changes(
        &self,
        id: ChangeID,
    ) -> Result<HashMap<BitVec<u8, Msb0>, Change>, BonsaiStorageError<DB::DatabaseError>> {
        self.trie.db_ref().get_changes(id)
    }

    #[cfg(test)]
    pub fn dump_database(&self) {
        self.trie.db_ref().db.dump_database();
    }

    /// Get trie root hash at the latest commit
    pub fn root_hash(&self) -> Result<BonsaiTrieHash, BonsaiStorageError<DB::DatabaseError>> {
        Ok(self.trie.root_hash())
    }

    /// This function must be used with transactional state only.
    /// Similar to `commit` but without optimizations.
    pub fn transactional_commit(
        &mut self,
        id: ChangeID,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        self.trie.commit()?;
        self.trie.db_mut().commit(id)?;
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
        key: &BitSlice<u8, Msb0>,
    ) -> Result<Vec<ProofNode>, BonsaiStorageError<DB::DatabaseError>> {
        self.trie.get_proof(key)
    }

    /// Verifies a merkle-proof for a given `key` and `value`.
    pub fn verify_proof(
        root: Felt,
        key: &BitSlice<u8, Msb0>,
        value: Felt,
        proofs: &[ProofNode],
    ) -> Option<Membership> {
        MerkleTree::<Pedersen, DB, ChangeID>::verify_proof(root, key, value, proofs)
    }
}

impl<ChangeID, DB, H> BonsaiStorage<ChangeID, DB, H>
where
    DB: BonsaiDatabase + BonsaiPersistentDatabase<ChangeID>,
    ChangeID: id::Id,
    H: StarkHash,
{
    /// Update trie and database using all changes since the last commit.
    pub fn commit(
        &mut self,
        id: ChangeID,
    ) -> Result<(), BonsaiStorageError<<DB as BonsaiDatabase>::DatabaseError>> {
        self.trie.commit()?;
        self.trie.db_mut().commit(id)?;
        self.trie.db_mut().create_snapshot(id);
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    /// Get a transactional state of the trie at a specific commit ID.
    ///
    /// Transactional state allow you to fetch a point-in-time state of the trie. You can
    /// apply changes to this state and merge it back into the main trie.
    pub fn get_transactional_state(
        &self,
        change_id: ChangeID,
        config: BonsaiStorageConfig,
    ) -> Result<
        Option<BonsaiStorage<ChangeID, DB::Transaction, H>>,
        BonsaiStorageError<<DB::Transaction as BonsaiDatabase>::DatabaseError>,
    > {
        if let Some(transaction) = self.trie.db_ref().get_transaction(change_id)? {
            Ok(Some(BonsaiStorage::new_from_transactional_state(
                transaction,
                config,
                change_id,
            )?))
        } else {
            Ok(None)
        }
    }

    /// Get a copy of the config that can be used to create a transactional state or a new bonsai storage.
    pub fn get_config(&self) -> BonsaiStorageConfig {
        self.trie.db_ref().get_config().into()
    }

    /// Merge a transactional state into the main trie.
    pub fn merge(
        &mut self,
        transactional_bonsai_storage: BonsaiStorage<ChangeID, DB::Transaction, H>,
    ) -> Result<(), BonsaiStorageError<<DB as BonsaiPersistentDatabase<ChangeID>>::DatabaseError>>
    {
        self.trie
            .db_mut()
            .merge(transactional_bonsai_storage.trie.db())
    }
}
