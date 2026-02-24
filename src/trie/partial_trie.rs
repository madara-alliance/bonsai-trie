use super::iterator::{PartialMerkleTreeTraverser, PartialNodeVisitor};
use super::{
    iterator::NoopPartialVisitor,
    merkle_node::{Node, NodeHandle},
    tree::{MerkleTree, NodeKey},
};
use crate::fmt;
use crate::trie::proof::{ProofNode, ProofVerificationError};
use crate::DBError;
use crate::MultiProof;
use crate::{
    error::BonsaiStorageError, id::Id, vec, BitSlice, BonsaiDatabase, ByteVec, KeyValueDB,
    ToString, Vec,
};
use core::marker::PhantomData;
use starknet_types_core::{felt::Felt, hash::StarkHash};

#[derive(Debug, thiserror::Error)]
pub enum PartialTrieError {
    #[error(transparent)]
    ProofVerificationError(#[from] ProofVerificationError),
    #[error("Node not found in storage or proof")]
    NodeNotFound,
    #[error("Key length is not equal to max height")]
    KeyLength,
    #[error("Set value is zero")]
    SetValueZero,
    #[error("Invalid node handle - expected hash but got in-memory node")]
    InvalidNodeHandle,
}

impl<DBE: DBError> From<PartialTrieError> for BonsaiStorageError<DBE> {
    fn from(err: PartialTrieError) -> Self {
        BonsaiStorageError::Trie(err.to_string())
    }
}

/// A partial Merkle-Patricia trie that supports proof-based operations.
///
/// Unlike a full `MerkleTree`, a `PartialTrie` can operate with incomplete data
/// by loading missing nodes from proofs. This is useful for:
/// - Forking from a remote network without downloading the entire state
/// - Operating on a subset of the trie while maintaining cryptographic integrity
///
/// Nodes are lazily loaded from proofs during traversal and cached in the
/// underlying `MerkleTree` for subsequent operations.
pub struct PartialTrie<H: StarkHash> {
    pub(crate) trie: MerkleTree<H>,
    _hasher: PhantomData<H>,
}

impl<H: StarkHash> fmt::Debug for PartialTrie<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PartialTrie")
            .field("trie", &self.trie)
            .field("nodes", &self.trie.nodes)
            .field("identifier", &self.trie.identifier)
            .field("death_row", &self.trie.death_row)
            .field("cache_leaf_modified", &self.trie.cache_leaf_modified)
            .finish()
    }
}

impl<H: StarkHash + Send + Sync> PartialTrie<H> {
    pub fn new(identifier: ByteVec, max_height: u8) -> Self {
        Self {
            trie: MerkleTree::new(identifier, max_height),
            _hasher: PhantomData,
        }
    }

    /// Sets a value in the partial-trie and returns the updated root.
    /// Uses proof to fetch missing nodes.
    /// On first call, traverses the entire proof to build the tree from scratch.
    pub fn set_with_proof<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &mut KeyValueDB<DB, ID>,
        key: &BitSlice,
        value: Felt,
        proof: &MultiProof,
        original_root: Felt,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        let path_nodes = self.seek_to(key, proof, original_root, db)?;

        self.trie.set_with_path_nodes(db, key, value, path_nodes)?;

        Ok(())
    }

    /// Traverses the current partial tree and collects existing elements.
    /// If the tree is empty, completes it from the proof.
    pub fn seek_to<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        key: &BitSlice,
        proof: &MultiProof,
        original_root: Felt,
        db: &KeyValueDB<DB, ID>,
    ) -> Result<Vec<(NodeKey, usize)>, BonsaiStorageError<DB::DatabaseError>> {
        let proof_keys = vec![key];
        let max_height = self.trie.max_height;
        let mut iter = self.trie.iter_partial_trie(db, proof);
        let mut visitor = NoopPartialVisitor::<H>(PhantomData);

        for key in proof_keys {
            let key = key.as_ref();
            if key.len() != max_height as usize {
                return Err(PartialTrieError::KeyLength.into());
            }
            iter.traverse_to::<NoopPartialVisitor<H>>(&mut visitor, key, original_root)?;
        }
        let path_nodes = iter.current_partial_nodes_heights;

        Ok(path_nodes)
    }

    /// # Panics
    ///
    /// Calling this function when the tree has uncommited changes is invalid as the hashes need to be recomputed.
    pub fn root_hash<DB: BonsaiDatabase, ID: Id>(
        &self,
        db: &KeyValueDB<DB, ID>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        self.trie.root_hash::<DB, ID>(db)
    }

    // Commit a single merkle tree
    #[cfg(test)]
    pub(crate) fn commit<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &mut KeyValueDB<DB, ID>,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        self.trie.commit::<DB, ID>(db)
    }

    pub fn get_multi_proof<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
        keys: impl IntoIterator<Item = impl AsRef<BitSlice>>,
    ) -> Result<MultiProof, BonsaiStorageError<DB::DatabaseError>> {
        self.get_multi_proof_partial_trie(db, keys, None, None)
    }

    /// Generates a multi-proof for the given keys.
    ///
    /// For keys that exist in the local database, nodes are loaded directly.
    /// For missing nodes, the `original_proof` is used as a fallback data source.
    /// Nodes loaded from the proof are cached in memory and persisted on `commit()`.
    ///
    /// # Arguments
    /// * `db` - Database connection for loading cached nodes
    /// * `keys` - Keys to generate proof for
    /// * `original_proof` - Fallback proof for loading missing nodes (from parent/forked state)
    /// * `original_root` - Root hash of the original tree (required when using original_proof)
    ///
    /// # Returns
    /// A `MultiProof` that can verify all requested keys against this trie's root.
    pub fn get_multi_proof_partial_trie<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
        keys: impl IntoIterator<Item = impl AsRef<BitSlice>>,
        original_proof: Option<MultiProof>,
        original_root: Option<Felt>,
    ) -> Result<MultiProof, BonsaiStorageError<DB::DatabaseError>> {
        let max_height = self.trie.max_height;

        struct PartialProofVisitor<H: StarkHash>(MultiProof, PhantomData<H>);
        impl<H: StarkHash + Send + Sync> PartialNodeVisitor<H> for PartialProofVisitor<H> {
            fn visit_partial_node<DB: BonsaiDatabase>(
                &mut self,
                tree: &mut MerkleTree<H>,
                node_id: NodeKey,
                _prev_height: usize,
            ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
                let proof_node = match tree.get_node_mut::<DB>(node_id)? {
                    Node::Binary(binary_node) => {
                        let (left, right) = (binary_node.left, binary_node.right);
                        ProofNode::Binary {
                            left: tree.get_or_compute_node_hash::<DB>(left)?,
                            right: tree.get_or_compute_node_hash::<DB>(right)?,
                        }
                    }
                    Node::Edge(edge_node) => {
                        let (child, path) = (edge_node.child, edge_node.path.clone());
                        ProofNode::Edge {
                            child: tree.get_or_compute_node_hash::<DB>(child)?,
                            path,
                        }
                    }
                };
                let hash = tree.get_or_compute_node_hash::<DB>(NodeHandle::InMemory(node_id))?;
                self.0 .0.insert(hash, proof_node);
                Ok(())
            }
        }

        // Use provided proof or empty proof - the iterator will load nodes from database or proof
        let proof = original_proof.unwrap_or_else(|| MultiProof(Default::default()));
        let mut iter = self.trie.iter_partial_trie(db, &proof);
        let mut visitor = PartialProofVisitor::<H>(MultiProof(Default::default()), PhantomData);

        // TODO: handle it better way instead of default
        let root_hash = original_root.unwrap_or_default();

        for key in keys {
            let key = key.as_ref();
            if key.len() != max_height as usize {
                return Err(BonsaiStorageError::KeyLength {
                    expected: self.trie.max_height as _,
                    got: key.len(),
                });
            }

            // Traverse to the key - visitor will be called automatically by iterator
            iter.traverse_to::<PartialProofVisitor<H>>(&mut visitor, key, root_hash)?;
        }

        Ok(visitor.0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        databases::{create_rocks_db, RocksDB, RocksDBConfig},
        id::{BasicId, BasicIdBuilder},
        BitVec, BonsaiStorage, BonsaiStorageConfig, PartialMerkleTrees,
    };
    use bitvec::{bits, prelude::Msb0};
    use proptest::collection::vec;
    use proptest::num::u8;
    use proptest::prelude::*;
    use starknet_types_core::hash::Pedersen;

    fn arb_key_value(max_height: u8) -> impl Strategy<Value = (BitVec, Felt)> {
        (
            prop::collection::vec(any::<bool>(), max_height as usize),
            (1..=254u8),
        )
            .prop_map(move |(bits, v)| {
                let mut key = BitVec::with_capacity(max_height as usize);
                key.extend(bits);
                debug_assert_eq!(
                    key.len(),
                    max_height as usize,
                    "Key length must match max_height"
                );
                let value = Felt::from(v as u64 + 1);
                (key, value)
            })
    }

    fn arb_power_of_two_keys(max_height: u8) -> impl Strategy<Value = Vec<(BitVec, Felt)>> {
        (0..4).prop_flat_map(move |power| {
            let num_keys = 1 << power;
            prop::collection::vec(arb_key_value(max_height), num_keys as usize)
        })
    }

    #[test]
    fn test_set_root_multiple_calls_single_test_height_8() {
        let base_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let base_db = create_rocks_db(&base_path).unwrap();

        let reference_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let reference_db = create_rocks_db(&reference_path).unwrap();

        let fork_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let fork_db = create_rocks_db(&fork_path).unwrap();

        let tree_to_compare_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let tree_to_compare_db = create_rocks_db(&tree_to_compare_path).unwrap();

        let identifier = vec![1];
        let identifier2 = vec![2];
        let identifier3 = vec![3];
        let identifier4 = vec![4];

        let config = BonsaiStorageConfig::default();
        let mut base_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&base_db, RocksDBConfig::default()),
                config.clone(),
                8,
            );
        let mut tree_to_compare: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&tree_to_compare_db, RocksDBConfig::default()),
                config.clone(),
                8,
            );
        let mut reference_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&reference_db, RocksDBConfig::default()),
                config.clone(),
                8,
            );
        let mut fork_tree: BonsaiStorage<
            BasicId,
            RocksDB<'_, BasicId>,
            Pedersen,
            PartialMerkleTrees<Pedersen, RocksDB<'_, BasicId>, BasicId>,
        > = BonsaiStorage::new_partial(
            RocksDB::new(&fork_db, RocksDBConfig::default()),
            config.clone(),
            8,
        );

        let mut id_builder = BasicIdBuilder::new();

        let one = bits![u8,   Msb0; 1,0,0,0,0,0,0,0];
        let two = bits![u8,   Msb0; 0,1,0,0,0,0,0,0];
        let three = bits![u8, Msb0; 1,1,0,0,0,0,0,0];
        let four = bits![u8,  Msb0; 0,0,1,0,0,0,0,0];
        let five = bits![u8,  Msb0; 1,0,1,0,0,0,0,0];
        let six = bits![u8,   Msb0; 0,1,1,0,0,0,0,0];
        let seven = bits![u8, Msb0; 1,1,1,0,0,0,0,0];
        let eight = bits![u8, Msb0; 0,0,0,1,0,0,0,0];
        let nine = bits![u8,  Msb0; 1,0,0,1,0,0,0,0];
        let ten = bits![u8,   Msb0; 0,0,0,0,1,0,1,0];
        let eleven = bits![u8,Msb0; 0,0,0,0,1,0,1,1];
        let twelve = bits![u8,Msb0; 0,0,0,0,1,1,0,0];

        let keys = vec![
            one, two, three, four, five, six, seven, eight, nine, ten, eleven, twelve,
        ];
        let values = vec![
            Felt::from(1),
            Felt::from(2),
            Felt::from(3),
            Felt::from(4),
            Felt::from(5),
            Felt::from(6),
            Felt::from(7),
            Felt::from(8),
            Felt::from(9),
            Felt::from(10),
            Felt::from(11),
            Felt::from(12),
        ];

        for (key, value) in keys.iter().zip(values.iter()).take(3) {
            base_tree.insert(&identifier, key, value).unwrap();
            reference_tree.insert(&identifier3, key, value).unwrap(); // Reference tree for comparison
        }

        base_tree.commit(id_builder.new_id()).unwrap();
        let original_root = base_tree.root_hash(&identifier).unwrap();

        let mut calculated_roots: Vec<Felt> = Vec::new();
        let mut current_root = original_root;

        let tree1 = base_tree
            .tries
            .trees
            .entry(smallvec::smallvec![1])
            .or_insert_with(|| MerkleTree::new(identifier.clone().into(), 8));

        let proof_key_one = vec![one];
        let proof_for_one = tree1
            .get_multi_proof(&base_tree.tries.db, proof_key_one.iter())
            .unwrap();

        for (key, value) in keys.iter().zip(values.iter()).skip(3) {
            let proof_keys = vec![key];
            let proof = tree1
                .get_multi_proof(&base_tree.tries.db, proof_keys.iter())
                .unwrap();

            reference_tree.insert(&identifier3, key, value).unwrap();
            reference_tree.commit(id_builder.new_id()).unwrap();

            fork_tree
                .insert_with_proof(&identifier4, key, value, &proof, original_root)
                .unwrap();
            fork_tree.commit(id_builder.new_id()).unwrap();
            let fork_hash = fork_tree.root_hash(&identifier4).unwrap();

            calculated_roots.push(fork_hash);
            current_root = fork_hash;
        }

        for (key, value) in keys.iter().zip(values.iter()) {
            tree_to_compare.insert(&identifier2, key, value).unwrap();
        }

        tree_to_compare.commit(id_builder.new_id()).unwrap();

        let actual_root = tree_to_compare.root_hash(&identifier2).unwrap();
        assert_eq!(current_root, actual_root, "Next root calculation failed");

        fork_tree
            .insert_with_proof(
                &identifier4,
                one,
                &Felt::from(13),
                &proof_for_one,
                original_root,
            )
            .unwrap();

        fork_tree.commit(id_builder.new_id()).unwrap();
        let calculated_updated_root = fork_tree.root_hash(&identifier4).unwrap();

        tree_to_compare
            .insert(&identifier2, one, &Felt::from(13))
            .unwrap();
        tree_to_compare.commit(id_builder.new_id()).unwrap();

        let actual_updated_root = tree_to_compare.root_hash(&identifier2).unwrap();
        assert_eq!(
            calculated_updated_root, actual_updated_root,
            "UPDATING ROOT FAILED"
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::default())]
        #[test]
        fn test_set_root_multiple_calls_height_2(
            initial_keys_values in arb_power_of_two_keys(2),
            new_keys_values in vec(arb_key_value(2), 1..3),
        ) {
            println!("initial_keys_values: {:?}", initial_keys_values);
            println!("new_keys_values: {:?}", new_keys_values);
            test_set_root_multiple_calls(2, initial_keys_values, new_keys_values);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::default())]
        #[test]
        fn test_set_root_multiple_calls_height_4(
            initial_keys_values in arb_power_of_two_keys(4),
            new_keys_values in vec(arb_key_value(4), 1..5),
        ) {
            println!("initial_keys_values: {:?}", initial_keys_values);
            println!("new_keys_values: {:?}", new_keys_values);
            test_set_root_multiple_calls(4, initial_keys_values, new_keys_values);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::default())]
        #[test]
        fn test_set_root_multiple_calls_height_8(
            initial_keys_values in arb_power_of_two_keys(8),
            new_keys_values in vec(arb_key_value(8), 1..20),
        ) {
            test_set_root_multiple_calls(8, initial_keys_values, new_keys_values);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::default())]
        #[test]
        fn test_set_root_multiple_calls_height_24(
            initial_keys_values in arb_power_of_two_keys(24),
            new_keys_values in vec(arb_key_value(24), 1..5),
        ) {
            test_set_root_multiple_calls(24, initial_keys_values, new_keys_values);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::default())]
        #[test]
        fn test_set_root_multiple_calls_height_251(
            initial_keys_values in arb_power_of_two_keys(251),
            new_keys_values in vec(arb_key_value(251), 1..5),
        ) {
            test_set_root_multiple_calls(251, initial_keys_values, new_keys_values);
        }
    }

    fn test_set_root_multiple_calls(
        height: u8,
        initial_keys_values: Vec<(BitVec, Felt)>,
        new_keys_values: Vec<(BitVec, Felt)>,
    ) {
        let base_db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
        let reference_db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
        let fork_db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();

        let base_identifier = vec![1];
        let reference_identifier = vec![2];
        let fork_identifier = vec![3];

        let config = BonsaiStorageConfig::default();
        let mut base_bonsai_storage: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&base_db, RocksDBConfig::default()),
                config.clone(),
                height,
            );
        let mut reference_bonsai_storage: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&reference_db, RocksDBConfig::default()),
                config.clone(),
                height,
            );
        let mut forked_bonsai_storage: BonsaiStorage<
            BasicId,
            RocksDB<'_, BasicId>,
            Pedersen,
            PartialMerkleTrees<Pedersen, RocksDB<'_, BasicId>, BasicId>,
        > = BonsaiStorage::new_partial(
            RocksDB::new(&fork_db, RocksDBConfig::default()),
            config.clone(),
            height,
        );

        let mut id_builder = BasicIdBuilder::new();

        for (key, value) in initial_keys_values.iter() {
            base_bonsai_storage
                .insert(&base_identifier, key, value)
                .unwrap();
            reference_bonsai_storage
                .insert(&reference_identifier, key, value)
                .unwrap();
            base_bonsai_storage.commit(id_builder.new_id()).unwrap();
            reference_bonsai_storage
                .commit(id_builder.new_id())
                .unwrap();
        }

        let original_root = base_bonsai_storage.root_hash(&base_identifier).unwrap();

        let mut calculated_roots = Vec::new();

        let tree1 = base_bonsai_storage
            .tries
            .trees
            .entry(smallvec::smallvec![1])
            .or_insert_with(|| MerkleTree::new(base_identifier.clone().into(), height));

        for (key, value) in new_keys_values.iter() {
            let proof_keys = vec![key];
            let proof = tree1
                .get_multi_proof(&base_bonsai_storage.tries.db, proof_keys.iter())
                .unwrap();

            forked_bonsai_storage
                .insert_with_proof(&fork_identifier, key, value, &proof, original_root)
                .unwrap();
            forked_bonsai_storage.commit(id_builder.new_id()).unwrap();
            let fork_hash = forked_bonsai_storage.root_hash(&fork_identifier).unwrap();

            calculated_roots.push(fork_hash);
        }

        for ((key, value), expected_root) in new_keys_values.iter().zip(calculated_roots) {
            reference_bonsai_storage
                .insert(&reference_identifier, key, value)
                .unwrap();
            reference_bonsai_storage
                .commit(id_builder.new_id())
                .unwrap();

            let actual_root = reference_bonsai_storage
                .root_hash(&reference_identifier)
                .unwrap();
            println!("Expected root: {:?}", expected_root);
            println!("Actual root: {:?}", actual_root);
            assert_eq!(
                expected_root, actual_root,
                "Expected root is not equal to actual root"
            );
        }
    }
    #[test]
    fn test_bonsai_partial_trie() {
        let base_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let base_db = create_rocks_db(&base_path).unwrap();

        let reference_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let reference_db = create_rocks_db(&reference_path).unwrap();

        let fork_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let fork_db = create_rocks_db(&fork_path).unwrap();

        let tree_to_compare_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let tree_to_compare_db = create_rocks_db(&tree_to_compare_path).unwrap();

        let identifier = vec![1];
        let identifier2 = vec![2];
        let identifier3 = vec![3];
        let identifier4 = vec![4];

        let config = BonsaiStorageConfig::default();
        let mut base_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&base_db, RocksDBConfig::default()),
                config.clone(),
                8,
            );
        let mut tree_to_compare: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&tree_to_compare_db, RocksDBConfig::default()),
                config.clone(),
                8,
            );
        let mut reference_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&reference_db, RocksDBConfig::default()),
                config.clone(),
                8,
            );
        let mut fork_tree: BonsaiStorage<
            BasicId,
            RocksDB<'_, BasicId>,
            Pedersen,
            PartialMerkleTrees<Pedersen, RocksDB<'_, BasicId>, BasicId>,
        > = BonsaiStorage::new_partial(
            RocksDB::new(&fork_db, RocksDBConfig::default()),
            config.clone(),
            8,
        );

        let mut id_builder = BasicIdBuilder::new();

        let one = bits![u8,   Msb0; 1,0,0,0,0,0,0,0];
        let two = bits![u8,   Msb0; 0,1,0,0,0,0,0,0];
        let three = bits![u8, Msb0; 1,1,0,0,0,0,0,0];
        let four = bits![u8,  Msb0; 0,0,1,0,0,0,0,0];
        let five = bits![u8,  Msb0; 1,0,1,0,0,0,0,0];
        let six = bits![u8,   Msb0; 0,1,1,0,0,0,0,0];
        let seven = bits![u8, Msb0; 1,1,1,0,0,0,0,0];
        let eight = bits![u8, Msb0; 0,0,0,1,0,0,0,0];
        let nine = bits![u8,  Msb0; 1,0,0,1,0,0,0,0];
        let ten = bits![u8,   Msb0; 0,0,0,0,1,0,1,0];
        let eleven = bits![u8,Msb0; 0,0,0,0,1,0,1,1];
        let twelve = bits![u8,Msb0; 0,0,0,0,1,1,0,0];

        let keys = vec![
            one, two, three, four, five, six, seven, eight, nine, ten, eleven, twelve,
        ];
        let values = vec![
            Felt::from(1),
            Felt::from(2),
            Felt::from(3),
            Felt::from(4),
            Felt::from(5),
            Felt::from(6),
            Felt::from(7),
            Felt::from(8),
            Felt::from(9),
            Felt::from(10),
            Felt::from(11),
            Felt::from(12),
        ];

        for (key, value) in keys.iter().zip(values.iter()).take(3) {
            base_tree.insert(&identifier, key, value).unwrap();
            reference_tree.insert(&identifier3, key, value).unwrap(); // Reference tree for comparison
        }

        base_tree.commit(id_builder.new_id()).unwrap();
        let original_root = base_tree.root_hash(&identifier).unwrap();

        let mut calculated_roots: Vec<Felt> = Vec::new();

        let tree1 = base_tree
            .tries
            .trees
            .entry(smallvec::smallvec![1])
            .or_insert_with(|| MerkleTree::new(identifier.clone().into(), 8));

        let proof_key_one = vec![one];
        let proof_for_one = tree1
            .get_multi_proof(&base_tree.tries.db, proof_key_one.iter())
            .unwrap();

        let mut current_root = original_root;

        for (key, value) in keys.iter().zip(values.iter()).skip(3) {
            let proof_keys = vec![key];

            let proof = tree1
                .get_multi_proof(&base_tree.tries.db, proof_keys.iter())
                .unwrap();

            reference_tree.insert(&identifier3, key, value).unwrap();
            reference_tree.commit(id_builder.new_id()).unwrap();

            fork_tree
                .insert_with_proof(&identifier4, key, value, &proof, original_root)
                .unwrap();
            fork_tree.commit(id_builder.new_id()).unwrap();
            let fork_hash = fork_tree.root_hash(&identifier4).unwrap();

            calculated_roots.push(fork_hash);
            current_root = fork_hash;
        }

        for (key, value) in keys.iter().zip(values.iter()) {
            tree_to_compare.insert(&identifier2, key, value).unwrap();
        }

        tree_to_compare.commit(id_builder.new_id()).unwrap();

        let actual_root = tree_to_compare.root_hash(&identifier2).unwrap();
        assert_eq!(current_root, actual_root, "Next root calculation failed");

        fork_tree
            .insert_with_proof(
                &identifier4,
                one,
                &Felt::from(13),
                &proof_for_one,
                original_root,
            )
            .unwrap();
        fork_tree.commit(id_builder.new_id()).unwrap();
        let calculated_updated_root = fork_tree.root_hash(&identifier4).unwrap();

        tree_to_compare
            .insert(&identifier2, one, &Felt::from(13))
            .unwrap();
        tree_to_compare.commit(id_builder.new_id()).unwrap();

        let actual_updated_root = tree_to_compare.root_hash(&identifier2).unwrap();
        assert_eq!(
            calculated_updated_root, actual_updated_root,
            "UPDATING ROOT FAILED"
        );
    }
    #[test]
    fn test_insert_with_proof_multi_proof_with_mainnet_keys() {
        use bitvec::view::AsBits;
        let base_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let base_db = create_rocks_db(&base_path).unwrap();

        let fork_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let fork_db = create_rocks_db(&fork_path).unwrap();

        let identifier = vec![1];
        let identifier2 = vec![2];

        let config = BonsaiStorageConfig::default();

        let mut base_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&base_db, RocksDBConfig::default()),
                config.clone(),
                251,
            );

        let mut fork_tree: BonsaiStorage<
            BasicId,
            RocksDB<'_, BasicId>,
            Pedersen,
            PartialMerkleTrees<Pedersen, RocksDB<'_, BasicId>, BasicId>,
        > = BonsaiStorage::new_partial(
            RocksDB::new(&fork_db, RocksDBConfig::default()),
            config.clone(),
            251,
        );

        let mut id_builder = BasicIdBuilder::new();

        let mainnet_genesis_key: BitVec =
            Felt::from_hex("0x7dc7899aa655b0aae51eadff6d801a58e97dd99cf4666ee59e704249e51adf2")
                .unwrap()
                .to_bytes_be()
                .as_bits()[5..]
                .to_owned();

        let mainnet_key =
            Felt::from_hex("0x70388df3dbdff1dac1f867fd5e418893daf4db7a44dea33824f66c924625358")
                .unwrap()
                .to_bytes_be()
                .as_bits()[5..]
                .to_owned();

        let fork_keys: Vec<BitVec> = vec![
            "0x718254e2758595671ae17a81506b88489ed5ab6ea4664cd36fcb2b14e970831",
            "0x37ced5be9b4c84415d796cdd2ccf841fc83dc56b27c9e1b5d2ff018ed925bb8",
        ]
        .iter()
        .map(|h| Felt::from_hex(h).unwrap().to_bytes_be().as_bits()[5..].to_owned())
        .collect();

        base_tree
            .insert(
                &identifier,
                &mainnet_genesis_key,
                &Felt::from_hex(
                    "0x1b97e0ef7f5c2f2b7483cda252a3accc7f917773fb69d4bd290f92770069aec",
                )
                .unwrap(),
            )
            .unwrap();
        base_tree.commit(id_builder.new_id()).unwrap();

        base_tree
            .insert(&identifier, &mainnet_key, &Felt::from(1))
            .unwrap();
        base_tree.commit(id_builder.new_id()).unwrap();
        let original_root = base_tree.root_hash(&identifier).unwrap();

        let tree1 = base_tree
            .tries
            .trees
            .get_mut(&smallvec::smallvec![1])
            .unwrap();
        let proof = tree1
            .get_multi_proof(&base_tree.tries.db, &fork_keys)
            .unwrap();

        let _verified_values = proof
            .verify_proof::<Pedersen>(
                tree1.root_hash(&base_tree.tries.db).unwrap(),
                fork_keys.iter(),
                251,
            )
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        for (i, key) in fork_keys.iter().enumerate() {
            println!("Inserting hash {}: {:?}", i + 1, key);
            fork_tree
                .insert_with_proof(&identifier2, key, &Felt::from(1), &proof, original_root)
                .unwrap();
            fork_tree.commit(id_builder.new_id()).unwrap();
        }
    }

    #[test]
    fn test_insert_with_proof_separate_proofs_with_mainnet_keys() {
        use bitvec::view::AsBits;
        let base_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let base_db = create_rocks_db(&base_path).unwrap();

        let fork_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let fork_db = create_rocks_db(&fork_path).unwrap();

        let identifier = vec![1];
        let identifier2 = vec![2];

        let config = BonsaiStorageConfig::default();

        let mut base_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&base_db, RocksDBConfig::default()),
                config.clone(),
                251,
            );

        let mut fork_tree: BonsaiStorage<
            BasicId,
            RocksDB<'_, BasicId>,
            Pedersen,
            PartialMerkleTrees<Pedersen, RocksDB<'_, BasicId>, BasicId>,
        > = BonsaiStorage::new_partial(
            RocksDB::new(&fork_db, RocksDBConfig::default()),
            config.clone(),
            251,
        );

        let mut id_builder = BasicIdBuilder::new();

        let mainnet_genesis_key: BitVec =
            Felt::from_hex("0x7dc7899aa655b0aae51eadff6d801a58e97dd99cf4666ee59e704249e51adf2")
                .unwrap()
                .to_bytes_be()
                .as_bits()[5..]
                .to_owned();

        let mainnet_key =
            Felt::from_hex("0x70388df3dbdff1dac1f867fd5e418893daf4db7a44dea33824f66c924625358")
                .unwrap()
                .to_bytes_be()
                .as_bits()[5..]
                .to_owned();

        let fork_keys: Vec<BitVec> = vec![
            "0x37ced5be9b4c84415d796cdd2ccf841fc83dc56b27c9e1b5d2ff018ed925bb8",
            "0x718254e2758595671ae17a81506b88489ed5ab6ea4664cd36fcb2b14e970831",
        ]
        .iter()
        .map(|h| Felt::from_hex(h).unwrap().to_bytes_be().as_bits()[5..].to_owned())
        .collect();

        base_tree
            .insert(
                &identifier,
                &mainnet_genesis_key,
                &Felt::from_hex(
                    "0x1b97e0ef7f5c2f2b7483cda252a3accc7f917773fb69d4bd290f92770069aec",
                )
                .unwrap(),
            )
            .unwrap();
        base_tree.commit(id_builder.new_id()).unwrap();

        base_tree
            .insert(&identifier, &mainnet_key, &Felt::from(1))
            .unwrap();
        base_tree.commit(id_builder.new_id()).unwrap();
        let original_root = base_tree.root_hash(&identifier).unwrap();

        let tree1 = base_tree
            .tries
            .trees
            .get_mut(&smallvec::smallvec![1])
            .unwrap();
        let proof = tree1
            .get_multi_proof(&base_tree.tries.db, vec![&fork_keys[0]])
            .unwrap();

        let proof2 = tree1
            .get_multi_proof(&base_tree.tries.db, vec![&fork_keys[1]])
            .unwrap();

        fork_tree
            .insert_with_proof(
                &identifier2,
                &fork_keys[0],
                &Felt::from(1),
                &proof,
                original_root,
            )
            .unwrap();
        fork_tree.commit(id_builder.new_id()).unwrap();

        fork_tree
            .insert_with_proof(
                &identifier2,
                &fork_keys[1],
                &Felt::from(1),
                &proof2,
                original_root,
            )
            .unwrap();
        fork_tree.commit(id_builder.new_id()).unwrap();
    }

    #[test]
    fn test_katana_multiproof_implementation() {
        use bitvec::view::AsBits;
        let base_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let base_db = create_rocks_db(&base_path).unwrap();
        let config = BonsaiStorageConfig::default();
        let identifier = vec![1];

        let mut base_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&base_db, RocksDBConfig::default()),
                config.clone(),
                251,
            );

        let mut id_builder = BasicIdBuilder::new();

        let mainnet_genesis_key: BitVec =
            Felt::from_hex("0x7dc7899aa655b0aae51eadff6d801a58e97dd99cf4666ee59e704249e51adf2")
                .unwrap()
                .to_bytes_be()
                .as_bits()[5..]
                .to_owned();

        let mainnet_key =
            Felt::from_hex("0x70388df3dbdff1dac1f867fd5e418893daf4db7a44dea33824f66c924625358")
                .unwrap()
                .to_bytes_be()
                .as_bits()[5..]
                .to_owned();

        let fork_keys: Vec<BitVec> = vec![
            "0x37ced5be9b4c84415d796cdd2ccf841fc83dc56b27c9e1b5d2ff018ed925bb8",
            "0x718254e2758595671ae17a81506b88489ed5ab6ea4664cd36fcb2b14e970831",
        ]
        .iter()
        .map(|h| Felt::from_hex(h).unwrap().to_bytes_be().as_bits()[5..].to_owned())
        .collect();

        base_tree
            .insert(
                &identifier,
                &mainnet_genesis_key,
                &Felt::from_hex(
                    "0x1b97e0ef7f5c2f2b7483cda252a3accc7f917773fb69d4bd290f92770069aec",
                )
                .unwrap(),
            )
            .unwrap();
        base_tree.commit(id_builder.new_id()).unwrap();

        base_tree
            .insert(&identifier, &mainnet_key, &Felt::from(1))
            .unwrap();
        base_tree.commit(id_builder.new_id()).unwrap();

        let tree1 = base_tree
            .tries
            .trees
            .get_mut(&smallvec::smallvec![1])
            .unwrap();
        let proof = tree1
            .get_multi_proof(&base_tree.tries.db, &fork_keys)
            .unwrap();

        // Verify proof before using
        let verified_values = proof
            .verify_proof::<Pedersen>(
                tree1.root_hash(&base_tree.tries.db).unwrap(),
                fork_keys.iter(),
                251,
            )
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        println!("Verified values: {:?}", verified_values);
    }

    #[test]
    fn test_insert_with_proof_multi_proof_with_mainnet_keys_state_updates() {
        use bitvec::view::AsBits;

        let base_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let base_db = create_rocks_db(&base_path).unwrap();

        let fork_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let fork_db = create_rocks_db(&fork_path).unwrap();

        let identifier = vec![1];
        let identifier2 = vec![2];

        let config = BonsaiStorageConfig::default();

        // OUTER TREE: address -> root(storage tree)
        let mut outer_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&base_db, RocksDBConfig::default()),
                config.clone(),
                251,
            );

        let mut fork_tree: BonsaiStorage<
            BasicId,
            RocksDB<'_, BasicId>,
            Pedersen,
            PartialMerkleTrees<Pedersen, RocksDB<'_, BasicId>, BasicId>,
        > = BonsaiStorage::new_partial(
            RocksDB::new(&fork_db, RocksDBConfig::default()),
            config.clone(),
            251,
        );
        let mut id_builder = BasicIdBuilder::new();

        let storage_updates: Vec<(Felt, Vec<(Felt, Felt)>)> = vec![
            (
                Felt::from_hex("0x1379ac0624b939ceb9dede92211d7db5ee174fe28be72245b0a1a2abd81c98f")
                    .unwrap(),
                vec![
                    (
                        Felt::from_hex(
                            "0x1501c0282d931e940cb3efce8df72c92216feadfac0b9163cc14261f80fa3a4",
                        )
                        .unwrap(),
                        Felt::from_hex(
                            "0x1501c0282d931e940cb3efce8df72c92216feadfac0b9163cc14261f80fa3a4",
                        )
                        .unwrap(),
                    ),
                    (
                        Felt::from_hex(
                            "0x78e6e3e4a50285be0f6e8d0b8a61044033e24023df6eb95979ae4073f159ae6",
                        )
                        .unwrap(),
                        Felt::from_hex(
                            "0x78e6e3e4a50285be0f6e8d0b8a61044033e24023df6eb95979ae4073f159ae6",
                        )
                        .unwrap(),
                    ),
                    (
                        Felt::from_hex(
                            "0x22796e1e0b20cd19185398001252dbbced3066054dbbab226c1d020a7e51fad",
                        )
                        .unwrap(),
                        Felt::from_hex(
                            "0x22796e1e0b20cd19185398001252dbbced3066054dbbab226c1d020a7e51fad",
                        )
                        .unwrap(),
                    ),
                    (
                        Felt::from_hex(
                            "0x2e5225082a276453856402ad3ed1921fd32a5b5f7ff0d723fb5f01963fdd7cf",
                        )
                        .unwrap(),
                        Felt::from_hex(
                            "0x2e5225082a276453856402ad3ed1921fd32a5b5f7ff0d723fb5f01963fdd7cf",
                        )
                        .unwrap(),
                    ),
                    (
                        Felt::from_hex(
                            "0x209b4f5c516c51edc98ba1ff716c61582a36bd9f1ec77d74fd194f45abc18e4",
                        )
                        .unwrap(),
                        Felt::from_hex(
                            "0x209b4f5c516c51edc98ba1ff716c61582a36bd9f1ec77d74fd194f45abc18e4",
                        )
                        .unwrap(),
                    ),
                    (
                        Felt::from_hex(
                            "0x3e642788efd4974adc7a73d0c0da0088ec55afeda0578ac185d60c2c0a8c243",
                        )
                        .unwrap(),
                        Felt::from_hex(
                            "0x3e642788efd4974adc7a73d0c0da0088ec55afeda0578ac185d60c2c0a8c243",
                        )
                        .unwrap(),
                    ),
                    (
                        Felt::from_hex(
                            "0x2f99396aad3919789352397b13bf620e00a30d72e364743043651d1f9dc81a2",
                        )
                        .unwrap(),
                        Felt::from_hex(
                            "0x2f99396aad3919789352397b13bf620e00a30d72e364743043651d1f9dc81a2",
                        )
                        .unwrap(),
                    ),
                    (
                        Felt::from_hex(
                            "0x2be7210355a6a885bc5e3a0a8d0c6668d861f72e0b80ce42a648927ac9fde8f",
                        )
                        .unwrap(),
                        Felt::from_hex(
                            "0x2be7210355a6a885bc5e3a0a8d0c6668d861f72e0b80ce42a648927ac9fde8f",
                        )
                        .unwrap(),
                    ),
                    (
                        Felt::from_hex(
                            "0x4eeeccfe0f96035b2576bdd3480ec340eb3150eca79dde7cfe574c573bb4be8",
                        )
                        .unwrap(),
                        Felt::from_hex(
                            "0x4eeeccfe0f96035b2576bdd3480ec340eb3150eca79dde7cfe574c573bb4be8",
                        )
                        .unwrap(),
                    ),
                    (
                        Felt::from_hex(
                            "0x42397031114e40d63d644b2bfb08b960218900d9ae8e33e7a9e0e3a5f8b9a3c",
                        )
                        .unwrap(),
                        Felt::from_hex(
                            "0x42397031114e40d63d644b2bfb08b960218900d9ae8e33e7a9e0e3a5f8b9a3c",
                        )
                        .unwrap(),
                    ),
                ],
            ),
            (
                Felt::from_hex("0xb6ce5410fca59d078ee9b2a4371a9d684c530d697c64fbef0ae6d5e8f0ac72")
                    .unwrap(),
                vec![
                    (
                        Felt::from_hex("0x5354524b").unwrap(),
                        Felt::from_hex("0x5354524b").unwrap(),
                    ),
                    (
                        Felt::from_hex("0x455448").unwrap(),
                        Felt::from_hex("0x455448").unwrap(),
                    ),
                ],
            ),
            (
                Felt::from_hex("0x110e2f729c9c2b988559994a3daccd838cf52faf88e18101373e67dd061455a")
                    .unwrap(),
                vec![
                    (
                        Felt::from_hex("0x152d02c7e14af6800000").unwrap(),
                        Felt::from_hex("0x152d02c7e14af6800000").unwrap(),
                    ),
                    (
                        Felt::from_hex("0x152d02c7e14af6800000").unwrap(),
                        Felt::from_hex("0x152d02c7e14af6800000").unwrap(),
                    ),
                ],
            ),
            (
                Felt::from_hex("0x110e2f729c9c2b988559994a3daccd838cf52faf88e18101373e67dd061455b")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x0").unwrap(),
                    Felt::from_hex("0x0").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x1ece7a660290ddd5c1d8cb8796de9f74e8a9b99ce52c3bc433a24b128d357d5")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x1ece7a660290ddd5c1d8cb8796de9f74e8a9b99ce52c3bc433a24b128d357d6")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x0").unwrap(),
                    Felt::from_hex("0x0").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x1f0d4aa99431d246bac9b8e48c33e888245b15e9678f64f9bdfc8823dc8f979")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x12").unwrap(),
                    Felt::from_hex("0x12").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x2a4a6011e8eeb2db7aab0c3d512034900714454f9bb4b015b423bd0038ff2c6")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x2a4a6011e8eeb2db7aab0c3d512034900714454f9bb4b015b423bd0038ff2c7")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x0").unwrap(),
                    Felt::from_hex("0x0").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x2dd9efcd0ced299772bb139967f74c09416fafc2057fe7370e37abe25b1917e")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x2dd9efcd0ced299772bb139967f74c09416fafc2057fe7370e37abe25b1917f")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x0").unwrap(),
                    Felt::from_hex("0x0").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x341c1bdfd89f69748aa00b5742b03adbffd79b8e80cab5c50d91cd8c2a79be1")
                    .unwrap(),
                vec![
                    (
                        Felt::from_hex("0x537461726b6e657420546f6b656e").unwrap(),
                        Felt::from_hex("0x537461726b6e657420546f6b656e").unwrap(),
                    ),
                    (
                        Felt::from_hex("0x4574686572").unwrap(),
                        Felt::from_hex("0x4574686572").unwrap(),
                    ),
                ],
            ),
            (
                Felt::from_hex("0x422ef899fa6ee66d84f0e6631f4c3a5a437bfb3528c556d7ee4f579ed738c5c")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x422ef899fa6ee66d84f0e6631f4c3a5a437bfb3528c556d7ee4f579ed738c5d")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x0").unwrap(),
                    Felt::from_hex("0x0").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x4a774c01093b1ef35cfe9809f95ace325718e1e031f8ab75286a677e274a239")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x4a774c01093b1ef35cfe9809f95ace325718e1e031f8ab75286a677e274a23a")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x0").unwrap(),
                    Felt::from_hex("0x0").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x4b24eca698466cf1cdf8c720348131e11626ae79ce304915876d7ab5352cfcb")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x4b24eca698466cf1cdf8c720348131e11626ae79ce304915876d7ab5352cfcc")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x0").unwrap(),
                    Felt::from_hex("0x0").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x614d03523bd9204d2b33d8601f39bd032af109785c861bbf8ab26c0fe899ef3")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x614d03523bd9204d2b33d8601f39bd032af109785c861bbf8ab26c0fe899ef4")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x0").unwrap(),
                    Felt::from_hex("0x0").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x6618020c2100ec29213b1c97c0e0a8c4355e508005baf2e46e4eeb926cec09f")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x6618020c2100ec29213b1c97c0e0a8c4355e508005baf2e46e4eeb926cec0a0")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x0").unwrap(),
                    Felt::from_hex("0x0").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x67840c21d0d3cba9ed504d8867dffe868f3d43708cfc0d7ed7980b511850070")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x67840c21d0d3cba9ed504d8867dffe868f3d43708cfc0d7ed7980b511850071")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x0").unwrap(),
                    Felt::from_hex("0x0").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x6c0ee267f85c6984a8633519c45c9d7e72d618ce8744940c3eeeb85a3aa7996")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                    Felt::from_hex("0x21e19e0c9bab2400000").unwrap(),
                )],
            ),
            (
                Felt::from_hex("0x6c0ee267f85c6984a8633519c45c9d7e72d618ce8744940c3eeeb85a3aa7997")
                    .unwrap(),
                vec![(
                    Felt::from_hex("0x0").unwrap(),
                    Felt::from_hex("0x0").unwrap(),
                )],
            ),
        ];

        for (address, storage_pairs) in &storage_updates {
            let mut storage_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
                BonsaiStorage::new(
                    RocksDB::new(&base_db, RocksDBConfig::default()),
                    config.clone(),
                    251,
                );

            for (storage_key, value) in storage_pairs {
                let key_bits = storage_key.to_bytes_be().as_bits()[5..].to_owned();
                storage_tree.insert(&identifier, &key_bits, value).unwrap();
            }

            storage_tree.commit(id_builder.new_id()).unwrap();
            let storage_root = storage_tree.root_hash(&identifier).unwrap();
            let address_bits = address.to_bytes_be().as_bits()[5..].to_owned();
            outer_tree
                .insert(&identifier, &address_bits, &storage_root)
                .unwrap();
        }
        outer_tree.commit(id_builder.new_id()).unwrap();
        let mainnet_root = outer_tree.root_hash(&identifier).unwrap();
        println!("Mainnet root: {:?}", mainnet_root);

        let initial_value =
            Felt::from_hex("0x4c3417b29b568b0ef3f6c1e4ab6aa844a26f7b6539f3853cae3c486e55f4774")
                .unwrap();
        let initial_value_bits = initial_value.to_bytes_be().as_bits()[5..].to_owned();
        outer_tree
            .insert(&identifier, &initial_value_bits, &Felt::from(0))
            .unwrap();
        outer_tree.commit(id_builder.new_id()).unwrap();

        let fork_keys: Vec<BitVec> = vec![
            "0x3d2d7cf3e9a59d09ed30e4812ab0d0cbd8cda5bdaa14a1bf5abe3ce6536ea7c",
            "0x246258999ea81791cf6e6873e9cffb15c27c4e96b99558d78ff7e3c177d73c8",
            "0x140d99b5f8493f04b1f1eb09734048e2860352cc76cd57f9b2e2a4deafbc9c0",
        ]
        .iter()
        .map(|h| Felt::from_hex(h).unwrap().to_bytes_be().as_bits()[5..].to_owned())
        .collect();

        let tree1 = outer_tree
            .tries
            .trees
            .get_mut(&smallvec::smallvec![1])
            .unwrap();
        let proof = tree1
            .get_multi_proof(&outer_tree.tries.db, &fork_keys)
            .unwrap();
        println!("Proof: {:?}", proof);

        let verified_values = proof
            .verify_proof::<Pedersen>(
                tree1.root_hash(&outer_tree.tries.db).unwrap(),
                fork_keys.iter(),
                251,
            )
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        println!("Verified values: {:?}", verified_values);

        for (i, key) in fork_keys.iter().enumerate() {
            let result = fork_tree.insert_with_proof(
                &identifier2,
                key,
                &Felt::from(0),
                &proof,
                mainnet_root,
            );
            match result {
                Ok(_) => {
                    fork_tree.commit(id_builder.new_id()).unwrap();
                    println!("Successfully inserted fork key {}", i + 1);
                }
                Err(e) => {
                    println!("Error inserting fork key {}: {:?}", i + 1, e);
                    panic!("Expected error occurred: {:?}", e);
                }
            }
        }
    }

    #[test]
    fn test_get_multi_proof_with_original_proof() {
        let _ = env_logger::builder().is_test(true).try_init();
        let base_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let base_db = create_rocks_db(&base_path).unwrap();

        let fork_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let fork_db = create_rocks_db(&fork_path).unwrap();

        let identifier = vec![1];
        let fork_identifier = vec![2];
        let config = BonsaiStorageConfig::default();

        let mut base_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&base_db, RocksDBConfig::default()),
                config.clone(),
                8,
            );

        let mut fork_tree: BonsaiStorage<
            BasicId,
            RocksDB<'_, BasicId>,
            Pedersen,
            PartialMerkleTrees<Pedersen, RocksDB<'_, BasicId>, BasicId>,
        > = BonsaiStorage::new_partial(
            RocksDB::new(&fork_db, RocksDBConfig::default()),
            config.clone(),
            8,
        );

        let mut id_builder = BasicIdBuilder::new();

        let key_a = bits![u8, Msb0; 0,0,0,1,0,0,0,0]; // A (modified)
        let key_b = bits![u8, Msb0; 0,0,0,1,0,0,0,1]; // B (only in base tree)
        let key_c = bits![u8, Msb0; 1,1,1,1,1,1,0,1]; // C (only in base tree)
        let key_d = bits![u8, Msb0; 1,0,0,1,0,0,0,1]; // D (only in fork)

        base_tree.insert(&identifier, key_a, &Felt::ONE).unwrap();
        base_tree.insert(&identifier, key_b, &Felt::TWO).unwrap();
        base_tree.insert(&identifier, key_c, &Felt::THREE).unwrap();
        base_tree.commit(id_builder.new_id()).unwrap();
        let original_root = base_tree.root_hash(&identifier).unwrap();

        let base_merkle_tree = base_tree
            .tries
            .trees
            .get_mut(&smallvec::smallvec![1])
            .unwrap();
        let original_proof_a = base_merkle_tree
            .get_multi_proof(&base_tree.tries.db, &[key_a])
            .unwrap();
        let original_proof_d = base_merkle_tree
            .get_multi_proof(&base_tree.tries.db, &[key_d])
            .unwrap();
        let original_proof_bc = base_merkle_tree
            .get_multi_proof(&base_tree.tries.db, &[key_b, key_c])
            .unwrap();

        fork_tree
            .insert_with_proof(
                &fork_identifier,
                key_a,
                &Felt::from(10),
                &original_proof_a,
                original_root,
            )
            .unwrap();
        fork_tree
            .insert_with_proof(
                &fork_identifier,
                key_d,
                &Felt::from(4),
                &original_proof_d,
                original_root,
            )
            .unwrap();
        fork_tree.commit(id_builder.new_id()).unwrap();

        let fork_merkle_tree = fork_tree
            .tries
            .trees
            .get_mut(&smallvec::smallvec![2])
            .unwrap();

        let fork_proof_bc = fork_merkle_tree
            .get_multi_proof_partial_trie(
                &fork_tree.tries.db,
                &[key_b, key_c],
                Some(original_proof_bc),
                Some(original_root),
            )
            .unwrap();

        let fork_root = fork_merkle_tree.root_hash(&fork_tree.tries.db).unwrap();
        let verified_bc = fork_proof_bc
            .verify_proof::<Pedersen>(fork_root, [key_b, key_c].iter(), 8)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(verified_bc[0], Felt::TWO, "B should be 2");
        assert_eq!(verified_bc[1], Felt::THREE, "C should be 3");
    }

    #[test]
    fn test_get_multi_proof_without_original_proof() {
        let tempdir = tempfile::tempdir().unwrap();
        let db = create_rocks_db(tempdir.path()).unwrap();
        let mut bonsai_storage: BonsaiStorage<BasicId, _, Pedersen> = BonsaiStorage::new(
            RocksDB::<BasicId>::new(&db, RocksDBConfig::default()),
            BonsaiStorageConfig::default(),
            8,
        );

        let key_values = [
            (bits![u8, Msb0; 0,0,0,1,0,0,0,0], Felt::ONE),
            (bits![u8, Msb0; 0,0,0,1,0,0,0,1], Felt::TWO),
            (bits![u8, Msb0; 0,0,0,1,1,1,0,1], Felt::THREE),
        ];

        for (k, v) in key_values.iter() {
            bonsai_storage.insert(&[], k, v).unwrap();
        }
        bonsai_storage
            .commit(BasicIdBuilder::new().new_id())
            .unwrap();

        let mut partial_trie = PartialTrie::<Pedersen>::new(vec![].into(), 8);
        for (k, v) in key_values.iter() {
            partial_trie
                .trie
                .set(&bonsai_storage.tries.db, k, *v)
                .unwrap();
        }
        partial_trie.commit(&mut bonsai_storage.tries.db).unwrap();

        let proof = partial_trie
            .get_multi_proof_partial_trie(
                &bonsai_storage.tries.db,
                key_values.iter().map(|(k, _v)| k),
                None, // No original_proof
                None, // No original_root
            )
            .unwrap();

        // Verify the proof
        let root_hash = partial_trie.root_hash(&bonsai_storage.tries.db).unwrap();
        let verified_values = proof
            .verify_proof::<Pedersen>(root_hash, key_values.iter().map(|(k, _v)| k), 8)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // Expected: All values match what we inserted
        assert_eq!(
            verified_values,
            key_values.iter().map(|(_k, v)| *v).collect::<Vec<_>>()
        );
    }

    /// Test get_multi_proof on a partial fork with only some keys modified.
    #[test]
    fn test_get_multi_proof_for_partial_fork() {
        let base_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let base_db = create_rocks_db(&base_path).unwrap();

        let fork_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let fork_db = create_rocks_db(&fork_path).unwrap();

        let identifier = vec![1];
        let fork_identifier = vec![2];
        let config = BonsaiStorageConfig::default();

        let mut base_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&base_db, RocksDBConfig::default()),
                config.clone(),
                8,
            );

        let mut fork_tree: BonsaiStorage<
            BasicId,
            RocksDB<'_, BasicId>,
            Pedersen,
            PartialMerkleTrees<Pedersen, RocksDB<'_, BasicId>, BasicId>,
        > = BonsaiStorage::new_partial(
            RocksDB::new(&fork_db, RocksDBConfig::default()),
            config.clone(),
            8,
        );

        let mut id_builder = BasicIdBuilder::new();

        // Setup: Base tree with 3 keys
        let key_a = bits![u8, Msb0; 0,0,0,1,0,0,0,0]; // A
        let key_b = bits![u8, Msb0; 0,0,0,1,0,0,0,1]; // B
        let key_c = bits![u8, Msb0; 0,1,1,1,1,1,0,1]; // C

        // Insert A, B, C into base tree
        base_tree.insert(&identifier, key_a, &Felt::ONE).unwrap();
        base_tree.insert(&identifier, key_b, &Felt::TWO).unwrap();
        base_tree.insert(&identifier, key_c, &Felt::THREE).unwrap();
        base_tree.commit(id_builder.new_id()).unwrap();
        let original_root = base_tree.root_hash(&identifier).unwrap();

        // Get proof from base tree for ALL keys (since fork will only have A)
        let base_merkle_tree = base_tree
            .tries
            .trees
            .get_mut(&smallvec::smallvec![1])
            .unwrap();
        let original_proof = base_merkle_tree
            .get_multi_proof(&base_tree.tries.db, &[key_a, key_b, key_c])
            .unwrap();

        // Fork: Modify only A=100 (B and C are NOT in fork's database!)
        fork_tree
            .insert_with_proof(
                &fork_identifier,
                key_a,
                &Felt::from(100),
                &original_proof,
                original_root,
            )
            .unwrap();
        fork_tree.commit(id_builder.new_id()).unwrap();

        // Generate multi-proof for ALL keys in fork using original_proof
        // This is the key test: fork only has A in database, but should generate
        // proof for B and C using original_proof
        let fork_merkle_tree = fork_tree
            .tries
            .trees
            .get_mut(&smallvec::smallvec![2])
            .unwrap();
        let fork_proof = fork_merkle_tree
            .get_multi_proof_partial_trie(
                &fork_tree.tries.db,
                &[key_a, key_b, key_c],
                Some(original_proof),
                Some(original_root),
            )
            .unwrap();

        // Verify the fork proof
        let fork_root = fork_merkle_tree.root_hash(&fork_tree.tries.db).unwrap();
        let verified_values = fork_proof
            .verify_proof::<Pedersen>(fork_root, [key_a, key_b, key_c].iter(), 8)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // Expected results:
        // A=100 (modified in fork, from fork database)
        // B=2 (unchanged, loaded from original_proof)
        // C=3 (unchanged, loaded from original_proof)
        assert_eq!(
            verified_values[0],
            Felt::from(100),
            "A should be 100 (modified in fork)"
        );
        assert_eq!(
            verified_values[1],
            Felt::TWO,
            "B should be 2 (from original_proof, not in fork DB)"
        );
        assert_eq!(
            verified_values[2],
            Felt::THREE,
            "C should be 3 (from original_proof, not in fork DB)"
        );
    }

    #[test]
    fn test_get_multi_proof_caches_nodes() {
        let base_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let base_db = create_rocks_db(&base_path).unwrap();

        let fork_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let fork_db = create_rocks_db(&fork_path).unwrap();

        let identifier = vec![1];
        let fork_identifier = vec![2];
        let config = BonsaiStorageConfig::default();

        let mut base_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&base_db, RocksDBConfig::default()),
                config.clone(),
                8,
            );

        let mut fork_tree: BonsaiStorage<
            BasicId,
            RocksDB<'_, BasicId>,
            Pedersen,
            PartialMerkleTrees<Pedersen, RocksDB<'_, BasicId>, BasicId>,
        > = BonsaiStorage::new_partial(
            RocksDB::new(&fork_db, RocksDBConfig::default()),
            config.clone(),
            8,
        );

        let mut id_builder = BasicIdBuilder::new();

        // Setup: Base tree with keys A, B, C
        let key_a = bits![u8, Msb0; 0,0,0,1,0,0,0,0]; // A
        let key_b = bits![u8, Msb0; 0,0,0,1,0,0,0,1]; // B
        let key_c = bits![u8, Msb0; 0,1,1,1,1,1,0,1]; // C

        base_tree.insert(&identifier, key_a, &Felt::ONE).unwrap();
        base_tree.insert(&identifier, key_b, &Felt::TWO).unwrap();
        base_tree.insert(&identifier, key_c, &Felt::THREE).unwrap();
        base_tree.commit(id_builder.new_id()).unwrap();
        let original_root = base_tree.root_hash(&identifier).unwrap();

        let base_merkle_tree = base_tree
            .tries
            .trees
            .get_mut(&smallvec::smallvec![1])
            .unwrap();
        let original_proof_a = base_merkle_tree
            .get_multi_proof(&base_tree.tries.db, &[key_a])
            .unwrap();
        let original_proof_bc = base_merkle_tree
            .get_multi_proof(&base_tree.tries.db, &[key_b, key_c])
            .unwrap();

        // Insert only A
        fork_tree
            .insert_with_proof(
                &fork_identifier,
                key_a,
                &Felt::from(10),
                &original_proof_a,
                original_root,
            )
            .unwrap();

        fork_tree.commit(id_builder.new_id()).unwrap();

        let fork_proof_1 = {
            let fork_merkle_tree = fork_tree
                .tries
                .trees
                .get_mut(&smallvec::smallvec![2])
                .unwrap();

            fork_merkle_tree
                .get_multi_proof_partial_trie(
                    &fork_tree.tries.db,
                    &[key_b, key_c],
                    Some(original_proof_bc.clone()), // Use original_proof
                    Some(original_root),
                )
                .unwrap()
        };

        // Verify first proof works
        let fork_root = fork_tree
            .tries
            .trees
            .get(&smallvec::smallvec![2])
            .unwrap()
            .root_hash(&fork_tree.tries.db)
            .unwrap();

        let verified_1 = fork_proof_1
            .verify_proof::<Pedersen>(fork_root, [key_b, key_c].iter(), 8)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(verified_1[0], Felt::TWO, "B should be 2");
        assert_eq!(verified_1[1], Felt::THREE, "C should be 3");

        // COMMIT: Save nodes from memory to database
        fork_tree.commit(id_builder.new_id()).unwrap();

        // SECOND CALL: Generate proof WITHOUT original_proof - nodes should be loaded from database
        let fork_proof_2 = {
            let fork_merkle_tree = fork_tree
                .tries
                .trees
                .get_mut(&smallvec::smallvec![2])
                .unwrap();

            fork_merkle_tree
                .get_multi_proof_partial_trie(
                    &fork_tree.tries.db,
                    &[key_b, key_c],
                    None, // No original_proof - should use nodes from database
                    Some(original_root),
                )
                .unwrap()
        };

        // Verify second proof works (using cached nodes from database)
        let fork_root_2 = fork_tree
            .tries
            .trees
            .get(&smallvec::smallvec![2])
            .unwrap()
            .root_hash(&fork_tree.tries.db)
            .unwrap();

        let verified_2 = fork_proof_2
            .verify_proof::<Pedersen>(fork_root_2, [key_b, key_c].iter(), 8)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            verified_2[0],
            Felt::TWO,
            "B should be 2 (from database cache)"
        );
        assert_eq!(
            verified_2[1],
            Felt::THREE,
            "C should be 3 (from database cache)"
        );

        // Verify that proofs are equivalent (same nodes, just loaded from different sources)
        assert_eq!(
            fork_proof_1.0.len(),
            fork_proof_2.0.len(),
            "Proofs should have same number of nodes"
        );
    }
}
