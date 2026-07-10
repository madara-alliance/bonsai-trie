use super::{proof::MultiProof, tree::MerkleTree};
use crate::{
    id::Id, key_value_db::KeyValueDB, trie::tree::InsertOrRemove, BitSlice, BitVec, BonsaiDatabase,
    BonsaiStorageError, ByteVec, HashMap, Vec,
};
use core::fmt;
use starknet_types_core::{felt::Felt, hash::StarkHash};

pub(crate) struct MerkleTrees<H: StarkHash + Send + Sync, DB: BonsaiDatabase, CommitID: Id> {
    pub db: KeyValueDB<DB, CommitID>,
    pub trees: HashMap<ByteVec, MerkleTree<H>>,
    pub max_height: u8,
}

impl<H: StarkHash + Send + Sync, DB: BonsaiDatabase + fmt::Debug, CommitID: Id> fmt::Debug
    for MerkleTrees<H, DB, CommitID>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MerkleTrees")
            .field("db", &self.db)
            .field("trees", &self.trees)
            .finish()
    }
}

#[cfg(feature = "bench")]
impl<H: StarkHash + Send + Sync, DB: BonsaiDatabase + Clone, CommitID: Id> Clone
    for MerkleTrees<H, DB, CommitID>
{
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            trees: self.trees.clone(),
            max_height: self.max_height,
        }
    }
}

impl<H: StarkHash + Send + Sync, DB: BonsaiDatabase, CommitID: Id> MerkleTrees<H, DB, CommitID> {
    pub(crate) fn new(db: KeyValueDB<DB, CommitID>, tree_height: u8) -> Self {
        Self {
            db,
            trees: HashMap::new(),
            max_height: tree_height,
        }
    }

    pub(crate) fn set(
        &mut self,
        identifier: &[u8],
        key: &BitSlice,
        value: Felt,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        let tree = self
            .trees
            .entry_ref(identifier)
            .or_insert_with(|| MerkleTree::new(identifier.into(), self.max_height));

        tree.set(&self.db, key, value)
    }

    pub(crate) fn set_owned(
        &mut self,
        identifier: &[u8],
        key: BitVec,
        value: Felt,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        let tree = self
            .trees
            .entry_ref(identifier)
            .or_insert_with(|| MerkleTree::new(identifier.into(), self.max_height));

        tree.set_owned(&self.db, key, value)
    }

    pub(crate) fn set_many_owned<I>(
        &mut self,
        identifier: &[u8],
        entries: I,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>>
    where
        I: IntoIterator<Item = (BitVec, Felt)>,
    {
        let tree = self
            .trees
            .entry_ref(identifier)
            .or_insert_with(|| MerkleTree::new(identifier.into(), self.max_height));

        tree.set_many_owned(&self.db, entries)
    }

    pub(crate) fn get(
        &self,
        identifier: &[u8],
        key: &BitSlice,
    ) -> Result<Option<Felt>, BonsaiStorageError<DB::DatabaseError>> {
        if let Some(tree) = self.trees.get(identifier) {
            tree.get(&self.db, key)
        } else {
            MerkleTree::<H>::new(identifier.into(), self.max_height).get(&self.db, key)
        }
    }

    pub(crate) fn get_at(
        &self,
        identifier: &[u8],
        key: &BitSlice,
        id: CommitID,
    ) -> Result<Option<Felt>, BonsaiStorageError<DB::DatabaseError>> {
        if let Some(tree) = self.trees.get(identifier) {
            tree.get_at(&self.db, key, id)
        } else {
            MerkleTree::<H>::new(identifier.into(), self.max_height).get_at(&self.db, key, id)
        }
    }

    pub(crate) fn contains(
        &self,
        identifier: &[u8],
        key: &BitSlice,
    ) -> Result<bool, BonsaiStorageError<DB::DatabaseError>> {
        if let Some(tree) = self.trees.get(identifier) {
            tree.contains(&self.db, key)
        } else {
            MerkleTree::<H>::new(identifier.into(), self.max_height).contains(&self.db, key)
        }
    }

    pub(crate) fn db_mut(&mut self) -> &mut KeyValueDB<DB, CommitID> {
        &mut self.db
    }

    pub(crate) fn reset_to_last_commit(
        &mut self,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        self.trees.clear(); // just clear the map
        Ok(())
    }

    pub(crate) fn db_ref(&self) -> &KeyValueDB<DB, CommitID> {
        &self.db
    }

    #[cfg(test)]
    pub fn dump(&self) {
        log::trace!("====== NUMBER OF TREES: {} ======", self.trees.len());
        self.trees.iter().for_each(|(k, tree)| {
            log::trace!("TREE identifier={:?}:", k);
            tree.dump();
        });
    }

    pub(crate) fn root_hash(
        &self,
        identifier: &[u8],
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        if let Some(tree) = self.trees.get(identifier) {
            Ok(tree.root_hash(&self.db)?)
        } else {
            MerkleTree::<H>::new(identifier.into(), self.max_height).root_hash(&self.db)
        }
    }

    /// Compute root hash from staged (uncommitted) changes. Falls back to the
    /// committed root when the identified tree has no pending modifications.
    pub(crate) fn root_hash_staged(
        &self,
        identifier: &[u8],
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        if let Some(tree) = self.trees.get(identifier) {
            tree.root_hash_staged(&self.db)
        } else {
            MerkleTree::<H>::new(identifier.into(), self.max_height).root_hash(&self.db)
        }
    }

    pub(crate) fn get_keys(
        &self,
        identifier: &[u8],
    ) -> Result<Vec<Vec<u8>>, BonsaiStorageError<DB::DatabaseError>> {
        self.db
            .db
            .get_by_prefix(&crate::DatabaseKey::Flat(identifier))
            .map(|key_value_pairs| {
                // Remove the identifier from the key
                key_value_pairs
                    .into_iter()
                    // FIXME: this does not filter out keys values correctly for `HashMapDb` due
                    // to branches and leafs not being differenciated
                    .filter_map(|(key, _value)| {
                        if key.len() > identifier.len() {
                            Some(key[identifier.len() + 1..].into())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .map_err(|e| e.into())
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn get_key_value_pairs(
        &self,
        identifier: &[u8],
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, BonsaiStorageError<DB::DatabaseError>> {
        self.db
            .db
            .get_by_prefix(&crate::DatabaseKey::Flat(identifier))
            .map(|key_value_pairs| {
                key_value_pairs
                    .into_iter()
                    // FIXME: this does not filter out keys values correctly for `HashMapDb` due
                    // to branches and leafs not being differenciated
                    .filter_map(|(key, value)| {
                        if key.len() > identifier.len() {
                            Some((key[identifier.len() + 1..].into(), value.into_vec()))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .map_err(|e| e.into())
    }

    pub(crate) fn commit(&mut self) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        #[cfg(feature = "std")]
        use rayon::prelude::*;

        #[cfg(feature = "std")]
        let get_updates_start = std::time::Instant::now();

        #[cfg(not(feature = "std"))]
        let db_changes = self
            .trees
            .iter_mut()
            .map(|(_, tree)| tree.get_updates::<DB>());
        #[cfg(feature = "std")]
        let db_changes = self
            .trees
            .par_iter_mut()
            .map(|(_, tree)| tree.get_updates::<DB>())
            .collect::<Vec<_>>();

        #[cfg(feature = "std")]
        let get_updates_duration = get_updates_start.elapsed();

        let track_changes = self.db.get_config().max_saved_trie_logs != Some(0);
        #[cfg(feature = "std")]
        let batch_prepare_start = std::time::Instant::now();

        let mut batch = self.db.create_batch();
        let mut total_updates = 0usize;
        let mut insert_updates = 0usize;
        let mut remove_updates = 0usize;
        for changes in db_changes {
            let changes = changes?;
            total_updates += changes.len();
            for (key, value) in changes.into_iter() {
                match value {
                    InsertOrRemove::Insert(value) => {
                        insert_updates += 1;
                        if track_changes {
                            self.db.insert(&key, &value, Some(&mut batch))?;
                        } else {
                            self.db.insert_untracked(&key, &value, &mut batch)?;
                        }
                    }
                    InsertOrRemove::Remove => {
                        remove_updates += 1;
                        if track_changes {
                            self.db.remove(&key, Some(&mut batch))?;
                        } else {
                            self.db.remove_untracked(&key, &mut batch)?;
                        }
                    }
                }
            }
        }

        #[cfg(feature = "std")]
        let batch_prepare_duration = batch_prepare_start.elapsed();
        #[cfg(feature = "std")]
        let write_batch_start = std::time::Instant::now();

        self.db.write_batch(batch)?;

        #[cfg(feature = "std")]
        log::info!(
            "bonsai merkle_trees commit timings trees={} updates={} inserts={} removes={} track_changes={} get_updates_ms={:.3} batch_prepare_ms={:.3} write_batch_ms={:.3}",
            self.trees.len(),
            total_updates,
            insert_updates,
            remove_updates,
            track_changes,
            get_updates_duration.as_secs_f64() * 1000.0,
            batch_prepare_duration.as_secs_f64() * 1000.0,
            write_batch_start.elapsed().as_secs_f64() * 1000.0,
        );
        Ok(())
    }

    // pub(crate) fn get_proof(
    //     &self,
    //     identifier: &[u8],
    //     key: &BitSlice,
    // ) -> Result<Vec<ProofNode>, BonsaiStorageError<DB::DatabaseError>> {
    //     if let Some(tree) = self.trees.get(identifier) {
    //         tree.get_proof(&self.db, key)
    //     } else {
    //         MerkleTree::<H>::new(identifier.into()).get_proof(&self.db, key)
    //     }
    // }

    pub fn get_multi_proof(
        &mut self,
        identifier: &[u8],
        keys: impl IntoIterator<Item = impl AsRef<BitSlice>>,
    ) -> Result<MultiProof, BonsaiStorageError<DB::DatabaseError>> {
        let tree = self
            .trees
            .entry_ref(identifier)
            .or_insert_with(|| MerkleTree::new(identifier.into(), self.max_height));

        tree.get_multi_proof(&self.db, keys)
    }
}
