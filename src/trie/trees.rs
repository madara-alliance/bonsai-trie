use super::{partial_trie::PartialTrie, proof::MultiProof, tree::MerkleTree};
use crate::{
    id::Id, key_value_db::KeyValueDB, trie::tree::InsertOrRemove, BitSlice, BonsaiDatabase,
    BonsaiStorageError, ByteVec, HashMap, Vec,
};
use core::fmt;
use starknet_types_core::{felt::Felt, hash::StarkHash};

/// Internal trait defining common operations for tree types (MerkleTree and PartialTrie).
pub trait TreeOperations<H: StarkHash, DB: BonsaiDatabase, CommitID: Id> {
    fn new(identifier: ByteVec, max_height: u8) -> Self;

    fn set(
        &mut self,
        db: &KeyValueDB<DB, CommitID>,
        key: &BitSlice,
        value: Felt,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>>;

    fn get(
        &self,
        db: &KeyValueDB<DB, CommitID>,
        key: &BitSlice,
    ) -> Result<Option<Felt>, BonsaiStorageError<DB::DatabaseError>>;

    fn get_at(
        &self,
        db: &KeyValueDB<DB, CommitID>,
        key: &BitSlice,
        id: CommitID,
    ) -> Result<Option<Felt>, BonsaiStorageError<DB::DatabaseError>>;

    fn contains(
        &self,
        db: &KeyValueDB<DB, CommitID>,
        key: &BitSlice,
    ) -> Result<bool, BonsaiStorageError<DB::DatabaseError>>;

    fn root_hash(
        &self,
        db: &KeyValueDB<DB, CommitID>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>>;

    fn get_updates(
        &mut self,
    ) -> Result<
        impl Iterator<Item = (crate::trie::TrieKey, InsertOrRemove<ByteVec>)>,
        BonsaiStorageError<DB::DatabaseError>,
    >;
}

impl<H: StarkHash + Send + Sync, DB: BonsaiDatabase, CommitID: Id> TreeOperations<H, DB, CommitID>
    for MerkleTree<H>
{
    fn new(identifier: ByteVec, max_height: u8) -> Self {
        MerkleTree::new(identifier, max_height)
    }

    fn set(
        &mut self,
        db: &KeyValueDB<DB, CommitID>,
        key: &BitSlice,
        value: Felt,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        self.set(db, key, value)
    }

    fn get(
        &self,
        db: &KeyValueDB<DB, CommitID>,
        key: &BitSlice,
    ) -> Result<Option<Felt>, BonsaiStorageError<DB::DatabaseError>> {
        self.get(db, key)
    }

    fn get_at(
        &self,
        db: &KeyValueDB<DB, CommitID>,
        key: &BitSlice,
        id: CommitID,
    ) -> Result<Option<Felt>, BonsaiStorageError<DB::DatabaseError>> {
        self.get_at(db, key, id)
    }

    fn contains(
        &self,
        db: &KeyValueDB<DB, CommitID>,
        key: &BitSlice,
    ) -> Result<bool, BonsaiStorageError<DB::DatabaseError>> {
        self.contains(db, key)
    }

    fn root_hash(
        &self,
        db: &KeyValueDB<DB, CommitID>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        self.root_hash(db)
    }

    fn get_updates(
        &mut self,
    ) -> Result<
        impl Iterator<Item = (crate::trie::TrieKey, InsertOrRemove<ByteVec>)>,
        BonsaiStorageError<DB::DatabaseError>,
    > {
        self.get_updates::<DB>()
    }
}

impl<H: StarkHash + Send + Sync, DB: BonsaiDatabase, CommitID: Id> TreeOperations<H, DB, CommitID>
    for PartialTrie<H>
{
    fn new(identifier: ByteVec, max_height: u8) -> Self {
        PartialTrie::new(identifier, max_height)
    }

    fn set(
        &mut self,
        db: &KeyValueDB<DB, CommitID>,
        key: &BitSlice,
        value: Felt,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        self.trie.set(db, key, value)
    }

    fn get(
        &self,
        db: &KeyValueDB<DB, CommitID>,
        key: &BitSlice,
    ) -> Result<Option<Felt>, BonsaiStorageError<DB::DatabaseError>> {
        self.trie.get(db, key)
    }

    fn get_at(
        &self,
        db: &KeyValueDB<DB, CommitID>,
        key: &BitSlice,
        id: CommitID,
    ) -> Result<Option<Felt>, BonsaiStorageError<DB::DatabaseError>> {
        self.trie.get_at(db, key, id)
    }

    fn contains(
        &self,
        db: &KeyValueDB<DB, CommitID>,
        key: &BitSlice,
    ) -> Result<bool, BonsaiStorageError<DB::DatabaseError>> {
        self.trie.contains(db, key)
    }

    fn root_hash(
        &self,
        db: &KeyValueDB<DB, CommitID>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        self.trie.root_hash(db)
    }

    fn get_updates(
        &mut self,
    ) -> Result<
        impl Iterator<Item = (crate::trie::TrieKey, InsertOrRemove<ByteVec>)>,
        BonsaiStorageError<DB::DatabaseError>,
    > {
        self.trie.get_updates::<DB>()
    }
}

pub struct MerkleTrees<
    H: StarkHash + Send + Sync,
    DB: BonsaiDatabase,
    CommitID: Id,
    TreeType = MerkleTree<H>,
> where
    TreeType: TreeOperations<H, DB, CommitID>,
{
    pub(crate) db: KeyValueDB<DB, CommitID>,
    pub(crate) trees: HashMap<ByteVec, TreeType>,
    pub(crate) max_height: u8,
    _phantom: core::marker::PhantomData<(H, DB, CommitID)>,
}

/// Type alias for full merkle trie storage (default).
pub type FullMerkleTrees<H, DB, CommitID> = MerkleTrees<H, DB, CommitID, MerkleTree<H>>;

/// Type alias for partial trie storage used in forked state scenarios.
pub type PartialMerkleTrees<H, DB, CommitID> = MerkleTrees<H, DB, CommitID, PartialTrie<H>>;

impl<H: StarkHash + Send + Sync, DB: BonsaiDatabase + fmt::Debug, CommitID: Id, TreeType> fmt::Debug
    for MerkleTrees<H, DB, CommitID, TreeType>
where
    TreeType: TreeOperations<H, DB, CommitID> + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MerkleTrees")
            .field("db", &self.db)
            .field("trees", &self.trees)
            .finish()
    }
}

#[cfg(feature = "bench")]
impl<H: StarkHash + Send + Sync, DB: BonsaiDatabase + Clone, CommitID: Id, TreeType> Clone
    for MerkleTrees<H, DB, CommitID, TreeType>
where
    TreeType: TreeOperations<H, DB, CommitID> + Clone,
{
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            trees: self.trees.clone(),
            max_height: self.max_height,
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<H: StarkHash + Send + Sync, DB: BonsaiDatabase, CommitID: Id, TreeType>
    MerkleTrees<H, DB, CommitID, TreeType>
where
    TreeType: TreeOperations<H, DB, CommitID>,
{
    pub(crate) fn new(db: KeyValueDB<DB, CommitID>, tree_height: u8) -> Self {
        Self {
            db,
            trees: HashMap::new(),
            max_height: tree_height,
            _phantom: core::marker::PhantomData,
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
            .or_insert_with(|| TreeType::new(identifier.into(), self.max_height));

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
            TreeType::new(identifier.into(), self.max_height).get(&self.db, key)
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
            TreeType::new(identifier.into(), self.max_height).get_at(&self.db, key, id)
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
            TreeType::new(identifier.into(), self.max_height).contains(&self.db, key)
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
    pub fn dump(&self)
    where
        TreeType: fmt::Debug,
    {
        log::trace!("====== NUMBER OF TREES: {} ======", self.trees.len());
        self.trees.iter().for_each(|(k, tree)| {
            log::trace!("TREE identifier={:?}:", k);
            log::trace!("{:?}", tree);
        });
    }

    pub(crate) fn root_hash(
        &self,
        identifier: &[u8],
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        if let Some(tree) = self.trees.get(identifier) {
            Ok(tree.root_hash(&self.db)?)
        } else {
            TreeType::new(identifier.into(), self.max_height).root_hash(&self.db)
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
        let mut batch = self.db.create_batch();

        for (_, tree) in self.trees.iter_mut() {
            let db_changes = tree.get_updates()?;
            for (key, value) in db_changes {
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
}

// Implementations specific to MerkleTree
impl<H: StarkHash + Send + Sync, DB: BonsaiDatabase, CommitID: Id>
    MerkleTrees<H, DB, CommitID, MerkleTree<H>>
{
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

// Implementations specific to PartialTrie
impl<H: StarkHash + Send + Sync, DB: BonsaiDatabase, CommitID: Id>
    MerkleTrees<H, DB, CommitID, PartialTrie<H>>
{
    pub(crate) fn get_multi_proof_partial_trie(
        &mut self,
        identifier: &[u8],
        keys: impl IntoIterator<Item = impl AsRef<BitSlice>>,
        original_proof: Option<MultiProof>,
        original_root: Option<Felt>,
    ) -> Result<MultiProof, BonsaiStorageError<DB::DatabaseError>> {
        let tree = self
            .trees
            .entry_ref(identifier)
            .or_insert_with(|| PartialTrie::new(identifier.into(), self.max_height));

        tree.get_multi_proof_partial_trie(&self.db, keys, original_proof, original_root)
    }
}
