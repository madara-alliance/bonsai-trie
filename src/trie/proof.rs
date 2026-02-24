use super::iterator::MerkleTreeTraverser;
use super::{
    merkle_node::{hash_binary_node, hash_edge_node, Direction},
    path::Path,
    tree::MerkleTree,
};
use crate::{
    id::Id,
    key_value_db::KeyValueDB,
    trie::{
        iterator::NodeVisitor,
        merkle_node::{Node, NodeHandle},
        tree::NodeKey,
    },
    BitSlice, BitVec, BonsaiDatabase, BonsaiStorageError, HashSet,
};
use core::{marker::PhantomData, mem};
use hashbrown::hash_set;
use indexmap::IndexMap;
use starknet_types_core::{felt::Felt, hash::StarkHash};

#[derive(Debug, thiserror::Error)]
pub enum ProofVerificationError {
    #[error("Key length mismatch: key {path:b}, expected length {expected}, got {got}")]
    KeyLengthMismatch {
        path: BitVec,
        expected: u8,
        got: usize,
    },
    #[error("Missing node in proof: key {path:b}, hash {hash:#x}")]
    MissingNode { path: BitVec, hash: Felt },
    #[error(
        "Overshot the expected path: path {path:b}, expected max height {expected_max_height}"
    )]
    Overshot {
        path: BitVec,
        expected_max_height: u8,
    },
    #[error("Node hash mismatch: path {path:b}, expected {expected:#x}, got {got:#x}")]
    HashMismatch {
        path: BitVec,
        expected: Felt,
        got: Felt,
    },
    #[error("Invalid proof")]
    InvalidProof,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProofNode {
    Binary { left: Felt, right: Felt },
    Edge { child: Felt, path: Path },
}

impl ProofNode {
    pub fn hash<H: StarkHash>(&self) -> Felt {
        match self {
            ProofNode::Binary { left, right } => hash_binary_node::<H>(*left, *right),
            ProofNode::Edge { child, path } => hash_edge_node::<H>(path, *child),
        }
    }
}

/// A multi-proof containing multiple proof nodes indexed by their hash.
/// Uses IndexMap to preserve insertion order during proof traversal.
#[derive(Debug, Clone)]
pub struct MultiProof(pub IndexMap<Felt, ProofNode>);
impl MultiProof {
    /// If the proof proves more than just the provided `key_values`, this function will not fail.
    /// Not the most optimized way of doing it, but we don't actually need to verify proofs in madara.
    /// As such, it has also not been properly proptested.
    ///
    /// Returns an iterator of the values. Felt::ZERO is returned when the key is not a member of the trie.
    /// Do not forget to check the values returned by the iterator :)
    pub fn verify_proof<'a, 'b: 'a, H: StarkHash>(
        &'b self,
        root: Felt,
        key_values: impl IntoIterator<Item = impl AsRef<BitSlice>> + 'a,
        tree_height: u8,
    ) -> impl Iterator<Item = Result<Felt, ProofVerificationError>> + 'a {
        let mut checked_cache: HashSet<Felt> = Default::default();
        let mut current_path = BitVec::with_capacity(251);
        key_values.into_iter().map(move |k| {
            let k = k.as_ref();

            if k.len() != tree_height as usize {
                return Err(ProofVerificationError::KeyLengthMismatch {
                    path: k.into(),
                    expected: tree_height,
                    got: k.len(),
                });
            }

            // Go down the tree, starting from the root
            current_path.clear(); // hoisted alloc
            let mut current_felt = root;

            loop {
                log::trace!("Start verify loop: {current_path:b} => {current_felt:#x}");
                if current_path.len() == k.len() {
                    // End of traversal, return value
                    log::trace!("End of traversal");
                    return Ok(current_felt);
                }
                if current_path.len() > k.len() {
                    // We overshot.
                    log::trace!("Overshot");
                    return Err(ProofVerificationError::Overshot {
                        path: mem::take(&mut current_path),
                        expected_max_height: tree_height,
                    });
                }
                let Some(node) = self.0.get(&current_felt) else {
                    // Missing node.
                    log::trace!("Missing");
                    return Err(ProofVerificationError::MissingNode {
                        path: mem::take(&mut current_path),
                        hash: current_felt,
                    });
                };

                // Check hash and save to verification cache.
                if let hash_set::Entry::Vacant(entry) = checked_cache.entry(current_felt) {
                    let computed_hash = node.hash::<H>();
                    if computed_hash != current_felt {
                        // Hash mismatch.
                        log::trace!("Hash mismatch: {computed_hash:#x} {current_felt:#x}");
                        return Err(ProofVerificationError::HashMismatch {
                            expected: current_felt,
                            got: computed_hash,
                            path: mem::take(&mut current_path),
                        });
                    }
                    entry.insert();
                }

                match node {
                    ProofNode::Binary { left, right } => {
                        // PANIC: We checked above that current_path.len() < k.len().
                        let direction = Direction::from(k[current_path.len()]);
                        log::trace!("Binary {direction:?}");
                        current_path.push(direction.into());
                        current_felt = match direction {
                            Direction::Left => *left,
                            Direction::Right => *right,
                        }
                    }
                    ProofNode::Edge { child, path } => {
                        log::trace!("Edge");
                        if k.get(current_path.len()..(current_path.len() + path.len()))
                            != Some(&path.0)
                        {
                            log::trace!("Wrong edge: {path:?}");
                            // Wrong edge path: that's a non-membership proof.
                            return Ok(Felt::ZERO);
                        }
                        current_path.extend_from_bitslice(&path.0);
                        current_felt = *child;
                    }
                }
            }
        })
    }
}

impl<H: StarkHash + Send + Sync> MerkleTree<H> {
    /// This function is designed to be very efficient if the `keys` are sorted - this allows for
    /// the minimal amount of backtracking when switching from one key to the next.
    pub fn get_multi_proof<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
        keys: impl IntoIterator<Item = impl AsRef<BitSlice>>,
    ) -> Result<MultiProof, BonsaiStorageError<DB::DatabaseError>> {
        let max_height = self.max_height;

        struct ProofVisitor<H>(MultiProof, PhantomData<H>);
        impl<H: StarkHash + Send + Sync> NodeVisitor<H> for ProofVisitor<H> {
            fn visit_node<DB: BonsaiDatabase>(
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
        let mut visitor = ProofVisitor::<H>(MultiProof(Default::default()), PhantomData);

        let mut iter = self.iter(db);
        for key in keys {
            let key = key.as_ref();
            if key.len() != max_height as usize {
                return Err(BonsaiStorageError::KeyLength {
                    expected: self.max_height as _,
                    got: key.len(),
                });
            }
            iter.traverse_to(&mut visitor, key)?;
        }

        Ok(visitor.0)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        databases::{create_rocks_db, RocksDB, RocksDBConfig},
        id::BasicId,
        BonsaiStorage, BonsaiStorageConfig,
    };
    use bitvec::{bits, order::Msb0};
    use starknet_types_core::{felt::Felt, hash::Pedersen};

    const ZERO: Felt = Felt::ZERO;
    const ONE: Felt = Felt::ONE;
    const TWO: Felt = Felt::TWO;
    const THREE: Felt = Felt::THREE;
    const FOUR: Felt = Felt::from_hex_unchecked("0x4");

    #[test]
    fn test_multiproof() {
        let _ = env_logger::builder().is_test(true).try_init();
        log::set_max_level(log::LevelFilter::Trace);
        let tempdir = tempfile::tempdir().unwrap();
        let db = create_rocks_db(tempdir.path()).unwrap();
        let mut bonsai_storage: BonsaiStorage<BasicId, _, Pedersen> = BonsaiStorage::new(
            RocksDB::<BasicId>::new(&db, RocksDBConfig::default()),
            BonsaiStorageConfig::default(),
            8,
        );

        let key_values = [
            (bits![u8, Msb0; 0,0,0,1,0,0,0,0], ONE),
            (bits![u8, Msb0; 0,0,0,1,0,0,0,1], TWO),
            (bits![u8, Msb0; 0,0,0,1,1,1,0,1], ZERO),
            (bits![u8, Msb0; 1,0,0,1,0,0,0,1], ZERO),
            (bits![u8, Msb0; 0,1,1,1,1,1,0,1], THREE),
            (bits![u8, Msb0; 0,0,0,1,0,0,1,0], ZERO),
            (bits![u8, Msb0; 0,1,0,0,0,0,0,0], FOUR),
            (bits![u8, Msb0; 1,0,0,1,0,1,0,1], ZERO),
        ];

        for (k, v) in key_values.iter() {
            bonsai_storage.insert(&[], k, v).unwrap();
        }

        bonsai_storage.dump();

        let tree = bonsai_storage
            .tries
            .trees
            .get_mut(&smallvec::smallvec![])
            .unwrap();

        let proof = tree
            .get_multi_proof(&bonsai_storage.tries.db, key_values.iter().map(|(k, _v)| k))
            .unwrap();

        log::trace!("proof: {proof:?}");
        assert_eq!(
            proof
                .verify_proof::<Pedersen>(
                    tree.root_hash(&bonsai_storage.tries.db).unwrap(),
                    key_values.iter().map(|(k, _v)| k),
                    8
                )
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            key_values.iter().map(|(_k, v)| *v).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_get_multi_proof_not_passes() {
        use crate::id::BasicIdBuilder;
        use crate::BitVec;
        use bitvec::view::AsBits;

        let base_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let base_db = create_rocks_db(&base_path).unwrap();

        let identifier = vec![1];

        let config = BonsaiStorageConfig::default();

        let mut base_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&base_db, RocksDBConfig::default()),
                config.clone(),
                251,
            );

        let mut id_builder = BasicIdBuilder::new();

        let key_1: BitVec =
            Felt::from_hex("0x7dc7899aa655b0aae51eadff6d801a58e97dd99cf4666ee59e704249e51adf2")
                .unwrap()
                .to_bytes_be()
                .as_bits()[5..]
                .to_owned();

        let key_2 =
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
            .insert(&identifier, &key_1, &Felt::from_hex("0x1").unwrap())
            .unwrap();
        base_tree.commit(id_builder.new_id()).unwrap();

        base_tree
            .insert(&identifier, &key_2, &Felt::from(1))
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

        let _verified_values = proof
            .verify_proof::<Pedersen>(
                tree1.root_hash(&base_tree.tries.db).unwrap(),
                fork_keys.iter(),
                251,
            )
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
    }

    #[test]
    fn test_get_multi_proof_passes() {
        use crate::id::BasicIdBuilder;
        use crate::BitVec;
        use bitvec::view::AsBits;

        let base_path = tempfile::tempdir().unwrap().path().to_path_buf();
        let base_db = create_rocks_db(&base_path).unwrap();

        let identifier = vec![1];

        let config = BonsaiStorageConfig::default();

        let mut base_tree: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
            BonsaiStorage::new(
                RocksDB::new(&base_db, RocksDBConfig::default()),
                config.clone(),
                251,
            );

        let mut id_builder = BasicIdBuilder::new();

        let key_1: BitVec =
            Felt::from_hex("0x7dc7899aa655b0aae51eadff6d801a58e97dd99cf4666ee59e704249e51adf2")
                .unwrap()
                .to_bytes_be()
                .as_bits()[5..]
                .to_owned();

        let key_2 =
            Felt::from_hex("0x70388df3dbdff1dac1f867fd5e418893daf4db7a44dea33824f66c924625358")
                .unwrap()
                .to_bytes_be()
                .as_bits()[5..]
                .to_owned();

        let fork_keys: Vec<BitVec> = vec![
            "0x718254e2758595671ae17a81506b88489ed5ab6ea4664cd36fcb2b14e970831",
            "0x37ced5be9b4c84415d796cdd2ccf841fc83dc56b27c9e1b5d2ff018ed925bb8",
            "0x140d99b5f8493f04b1f1eb09734048e2860352cc76cd57f9b2e2a4deafbc9c0",
        ]
        .iter()
        .map(|h| Felt::from_hex(h).unwrap().to_bytes_be().as_bits()[5..].to_owned())
        .collect();

        base_tree
            .insert(&identifier, &key_1, &Felt::from_hex("0x1").unwrap())
            .unwrap();
        base_tree.commit(id_builder.new_id()).unwrap();

        base_tree
            .insert(&identifier, &key_2, &Felt::from(1))
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

        let _verified_values = proof
            .verify_proof::<Pedersen>(
                tree1.root_hash(&base_tree.tries.db).unwrap(),
                fork_keys.iter(),
                251,
            )
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
    }
}
