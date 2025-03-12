use super::{proof::MultiProof, tree::MerkleTree};
use crate::{
    id::Id, key_value_db::KeyValueDB, trie::tree::InsertOrRemove, BitSlice, BonsaiDatabase,
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
            .collect_vec_list()
            .into_iter()
            .flatten();

        let mut batch = self.db.create_batch();
        for changes in db_changes {
            for (key, value) in changes? {
                match value {
                    InsertOrRemove::Insert(value) => {
                        self.db.insert(&key, &value, Some(&mut batch))?;
                    }
                    InsertOrRemove::Remove => {
                        self.db.remove(&key, Some(&mut batch))?;
                    }
                }
            }
        }
        self.db.write_batch(batch)?;
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
