use core::marker::PhantomData;

use super::{path::Path, tree::MerkleTree};
use crate::{
    id::Id,
    key_value_db::KeyValueDB,
    trie::{
        iterator::NodeVisitor,
        merkle_node::{Node, NodeHandle},
        tree::NodeKey,
    },
    BitSlice, BonsaiDatabase, BonsaiStorageError, HashMap,
};
use bitvec::view::BitView;
use starknet_types_core::{felt::Felt, hash::StarkHash};

#[derive(Debug, PartialEq, Eq)]
pub enum Membership {
    Member,
    NonMember,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProofNode {
    Binary { left: Felt, right: Felt },
    Edge { child: Felt, path: Path },
}

impl ProofNode {
    pub fn hash<H: StarkHash>(&self) -> Felt {
        match self {
            ProofNode::Binary { left, right } => H::hash(left, right),
            ProofNode::Edge { child, path } => {
                let mut bytes = [0u8; 32];
                bytes.view_bits_mut()[256 - path.0.len()..].copy_from_bitslice(&path.0);
                // SAFETY: path len is <= 251
                let path_hash = Felt::from_bytes_be(&bytes);

                let length = Felt::from(path.0.len() as u8);
                H::hash(child, &path_hash) + length
            }
        }
    }
}

impl<H: StarkHash + Send + Sync> MerkleTree<H> {
    pub fn get_multi_proof<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
        keys: impl IntoIterator<Item = impl AsRef<BitSlice>>,
    ) -> Result<HashMap<Felt, ProofNode>, BonsaiStorageError<DB::DatabaseError>> {
        struct ProofVisitor<H>(HashMap<Felt, ProofNode>, PhantomData<H>);
        impl<H: StarkHash + Send + Sync> NodeVisitor<H> for ProofVisitor<H> {
            fn visit_node<DB: BonsaiDatabase>(
                &mut self,
                tree: &mut MerkleTree<H>,
                node_id: NodeKey,
                _prev_height: usize,
            ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
                let proof_node = match tree.node_storage.get_node_mut::<DB>(node_id)? {
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
                self.0.insert(hash, proof_node);
                Ok(())
            }
        }
        let mut visitor = ProofVisitor::<H>(Default::default(), PhantomData);

        let mut iter = self.iter(db);
        for key in keys {
            iter.traverse_to(&mut visitor, key.as_ref())?;
        }

        Ok(visitor.0)
    }

    /// Function that come from pathfinder_merkle_tree::merkle_tree::MerkleTree
    /// Verifies that the key `key` with value `value` is indeed part of the MPT that has root
    /// `root`, given `proofs`.
    /// Supports proofs of non-membership as well as proof of membership: this function returns
    /// an enum corresponding to the membership of `value`, or returns `None` in case of a hash mismatch.
    /// The algorithm follows this logic:
    /// 1. init expected_hash <- root hash
    /// 2. loop over nodes: current <- nodes[i]
    ///    1. verify the current node's hash matches expected_hash (if not then we have a bad proof)
    ///    2. move towards the target - if current is:
    ///       1. binary node then choose the child that moves towards the target, else if
    ///       2. edge node then check the path against the target bits
    ///          1. If it matches then proceed with the child, else
    ///          2. if it does not match then we now have a proof that the target does not exist
    ///    3. nibble off target bits according to which child you got in (2). If all bits are gone then you
    ///       have reached the target and the child hash is the value you wanted and the proof is complete.
    ///    4. set expected_hash <- to the child hash
    /// 3. check that the expected_hash is `value` (we should've reached the leaf)
    pub fn verify_proof(
        _root: Felt,
        _key: &BitSlice,
        _value: Felt,
        _proofs: &[ProofNode],
    ) -> Option<Membership> {
        todo!()
        // Protect from ill-formed keys
        // if key.len() > 251 {
        //     return None;
        // }

        // let mut expected_hash = root;
        // let mut remaining_path: &BitSlice = key;

        // for proof_node in proofs.iter() {
        //     // Hash mismatch? Return None.
        //     if proof_node.hash::<H>() != expected_hash {
        //         return None;
        //     }
        //     match proof_node {
        //         ProofNode::Binary { left, right } => {
        //             // Direction will always correspond to the 0th index
        //             // because we're removing bits on every iteration.
        //             let direction = Direction::from(remaining_path[0]);

        //             // Set the next hash to be the left or right hash,
        //             // depending on the direction
        //             expected_hash = match direction {
        //                 Direction::Left => *left,
        //                 Direction::Right => *right,
        //             };

        //             // Advance by a single bit
        //             remaining_path = &remaining_path[1..];
        //         }
        //         ProofNode::Edge { child, path } => {
        //             if path.0 != remaining_path[..path.0.len()] {
        //                 // If paths don't match, we've found a proof of non membership because we:
        //                 // 1. Correctly moved towards the target insofar as is possible, and
        //                 // 2. hashing all the nodes along the path does result in the root hash, which means
        //                 // 3. the target definitely does not exist in this tree
        //                 return Some(Membership::NonMember);
        //             }

        //             // Set the next hash to the child's hash
        //             expected_hash = *child;

        //             // Advance by the whole edge path
        //             remaining_path = &remaining_path[path.0.len()..];
        //         }
        //     }
        // }

        // // At this point, we should reach `value` !
        // if expected_hash == value {
        //     Some(Membership::Member)
        // } else {
        //     // Hash mismatch. Return `None`.
        //     None
        // }
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
        )
        .unwrap();

        bonsai_storage
            .insert(&[], bits![u8, Msb0; 0,0,0,1,0,0,0,0], &ONE)
            .unwrap();
        bonsai_storage
            .insert(&[], bits![u8, Msb0; 0,0,0,1,0,0,0,1], &TWO)
            .unwrap();
        bonsai_storage
            .insert(&[], bits![u8, Msb0; 0,0,0,1,0,0,1,0], &THREE)
            .unwrap();
        bonsai_storage
            .insert(&[], bits![u8, Msb0; 0,1,0,0,0,0,0,0], &FOUR)
            .unwrap();

        bonsai_storage.dump();

        let tree = bonsai_storage
            .tries
            .trees
            .get_mut(&smallvec::smallvec![])
            .unwrap();

        let proof = tree.get_multi_proof(&bonsai_storage.tries.db, [
            bits![u8, Msb0; 0,0,0,1,0,0,0,1],
            bits![u8, Msb0; 0,1,0,0,0,0,0,0],
            ])
            .unwrap();

        log::trace!("proof: {proof:?}");
        todo!()
    }
}
