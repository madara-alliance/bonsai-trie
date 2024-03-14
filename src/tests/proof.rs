#![cfg(feature = "std")]
use bitvec::vec::BitVec;
use pathfinder_common::{hash::PedersenHash, trie::TrieNode};
use pathfinder_crypto::Felt as PathfinderFelt;
use pathfinder_merkle_tree::tree::{MerkleTree, TestStorage};
use pathfinder_storage::{Node, StoredNode};
use rand::Rng;
use starknet_types_core::{felt::Felt, hash::Pedersen};
use std::collections::HashMap;

use crate::{
    databases::{create_rocks_db, RocksDB, RocksDBConfig},
    id::{BasicId, BasicIdBuilder},
    trie::merkle_tree::{Membership, ProofNode},
    BonsaiStorage, BonsaiStorageConfig,
};

/// Commits the tree changes and persists them to storage.
fn commit_and_persist(
    tree: MerkleTree<PedersenHash, 251>,
    storage: &mut TestStorage,
) -> (PathfinderFelt, u64) {
    use pathfinder_storage::Child;

    for (key, value) in &tree.leaves {
        let key = PathfinderFelt::from_bits(key).unwrap();
        storage.leaves.insert(key, *value);
    }

    let update = tree.commit(storage).unwrap();

    let mut indices = HashMap::new();
    let mut idx = storage.nodes.len();
    for hash in update.nodes.keys() {
        indices.insert(*hash, idx as u64);
        idx += 1;
    }

    for (hash, node) in update.nodes {
        let node = match node {
            Node::Binary { left, right } => {
                let left = match left {
                    Child::Id(idx) => idx,
                    Child::Hash(hash) => {
                        *indices.get(&hash).expect("Left child should have an index")
                    }
                };

                let right = match right {
                    Child::Id(idx) => idx,
                    Child::Hash(hash) => *indices
                        .get(&hash)
                        .expect("Right child should have an index"),
                };

                StoredNode::Binary { left, right }
            }
            Node::Edge { child, path } => {
                let child = match child {
                    Child::Id(idx) => idx,
                    Child::Hash(hash) => *indices.get(&hash).expect("Child should have an index"),
                };

                StoredNode::Edge { child, path }
            }
            Node::LeafBinary => StoredNode::LeafBinary,
            Node::LeafEdge { path } => StoredNode::LeafEdge { path },
        };

        storage
            .nodes
            .insert(*indices.get(&hash).unwrap(), (hash, node));
    }

    let index = *indices.get(&update.root).unwrap();

    (update.root, index)
}

fn assert_eq_proof(bonsai_proof: &[ProofNode], pathfinder_proof: &[TrieNode]) {
    for (bonsai_node, pathfinder_node) in bonsai_proof.iter().zip(pathfinder_proof.iter()) {
        match (bonsai_node, pathfinder_node) {
            (
                ProofNode::Binary { left, right },
                pathfinder_common::trie::TrieNode::Binary {
                    left: pathfinder_left,
                    right: pathfinder_right,
                },
            ) => {
                let pathfinder_left_bits = pathfinder_left.to_hex_str();
                let pathfinder_felt = Felt::from_hex(&pathfinder_left_bits).unwrap();
                assert_eq!(left, &pathfinder_felt);
                let pathfinder_right_bits = pathfinder_right.to_hex_str();
                let pathfinder_felt = Felt::from_hex(&pathfinder_right_bits).unwrap();
                assert_eq!(right, &pathfinder_felt);
            }
            (
                ProofNode::Edge { child, path },
                pathfinder_common::trie::TrieNode::Edge {
                    child: pathfinder_child,
                    path: pathfinder_path,
                },
            ) => {
                let pathfinder_child_bits = pathfinder_child.to_hex_str();
                let pathfinder_felt = Felt::from_hex(&pathfinder_child_bits).unwrap();
                assert_eq!(child, &pathfinder_felt);
                assert_eq!(&path.0, pathfinder_path);
            }
            _ => panic!("Proofs are not the same"),
        }
    }
}

#[test]
fn basic_proof() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut storage = pathfinder_merkle_tree::tree::TestStorage::default();
    let mut bonsai_storage =
        BonsaiStorage::<_, _, Pedersen>::new(RocksDB::new(&db, RocksDBConfig::default()), config)
            .unwrap();
    let mut pathfinder_merkle_tree: MerkleTree<PedersenHash, 251> =
        pathfinder_merkle_tree::tree::MerkleTree::empty();
    let mut id_builder = BasicIdBuilder::new();
    let pair1 = (
        vec![1, 2, 1],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    pathfinder_merkle_tree
        .set(
            &storage,
            bitvec,
            PathfinderFelt::from_hex_str("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
        )
        .unwrap();
    let pair2 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair2.1)
        .unwrap();
    pathfinder_merkle_tree
        .set(
            &storage,
            bitvec,
            PathfinderFelt::from_hex_str("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
        )
        .unwrap();
    let pair3 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair3.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair3.1)
        .unwrap();
    pathfinder_merkle_tree
        .set(
            &storage,
            bitvec,
            PathfinderFelt::from_hex_str("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
        )
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let (_, root_id) = commit_and_persist(pathfinder_merkle_tree.clone(), &mut storage);
    let bonsai_proof = bonsai_storage
        .get_proof(&identifier, &BitVec::from_vec(vec![1, 2, 1]))
        .unwrap();
    let pathfinder_proof =
        pathfinder_merkle_tree::tree::MerkleTree::<PedersenHash, 251>::get_proof(
            root_id,
            &storage,
            &BitVec::from_vec(vec![1, 2, 1]),
        )
        .unwrap();
    assert_eq_proof(&bonsai_proof, &pathfinder_proof);
    assert_eq!(
        BonsaiStorage::<BasicId, RocksDB<BasicId>, Pedersen>::verify_proof(
            bonsai_storage.root_hash(&identifier).unwrap(),
            &BitVec::from_vec(vec![1, 2, 1]),
            pair1.1,
            &bonsai_proof
        ),
        Some(Membership::Member)
    );
}

#[test]
fn multiple_proofs() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut storage = pathfinder_merkle_tree::tree::TestStorage::default();
    let mut bonsai_storage =
        BonsaiStorage::<_, _, Pedersen>::new(RocksDB::new(&db, RocksDBConfig::default()), config)
            .unwrap();
    let mut pathfinder_merkle_tree: MerkleTree<PedersenHash, 251> =
        pathfinder_merkle_tree::tree::MerkleTree::empty();
    let mut id_builder = BasicIdBuilder::new();
    let mut rng = rand::thread_rng();
    let tree_size = rng.gen_range(10..1000);
    let mut elements = vec![];
    for _ in 0..tree_size {
        let mut element = String::from("0x");
        let element_size = rng.gen_range(10..32);
        for _ in 0..element_size {
            let random_byte: u8 = rng.gen();
            element.push_str(&format!("{:02x}", random_byte));
        }
        let value = Felt::from_hex(&element).unwrap();
        let key = &value.to_bytes_be()[..31];
        bonsai_storage
            .insert(&identifier, &BitVec::from_vec(key.to_vec()), &value)
            .unwrap();
        pathfinder_merkle_tree
            .set(
                &storage,
                BitVec::from_vec(key.to_vec()),
                PathfinderFelt::from_hex_str(&element).unwrap(),
            )
            .unwrap();
        elements.push((key.to_vec(), value));
    }
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let (_, root_id) = commit_and_persist(pathfinder_merkle_tree.clone(), &mut storage);
    for element in elements.iter() {
        let proof = bonsai_storage
            .get_proof(&identifier, &BitVec::from_vec(element.0.clone()))
            .unwrap();
        let pathfinder_proof =
            pathfinder_merkle_tree::tree::MerkleTree::<PedersenHash, 251>::get_proof(
                root_id,
                &storage,
                &BitVec::from_vec(element.0.clone()),
            )
            .unwrap();
        assert_eq_proof(&proof, &pathfinder_proof);
        assert_eq!(
            BonsaiStorage::<BasicId, RocksDB<BasicId>, Pedersen>::verify_proof(
                bonsai_storage.root_hash(&identifier).unwrap(),
                &BitVec::from_vec(element.0.clone()),
                element.1,
                &proof
            ),
            Some(Membership::Member)
        );
    }
}

#[test]
fn one_element_proof() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut storage = pathfinder_merkle_tree::tree::TestStorage::default();
    let mut bonsai_storage =
        BonsaiStorage::<_, _, Pedersen>::new(RocksDB::new(&db, RocksDBConfig::default()), config)
            .unwrap();
    let mut pathfinder_merkle_tree: MerkleTree<PedersenHash, 251> =
        pathfinder_merkle_tree::tree::MerkleTree::empty();
    let mut id_builder = BasicIdBuilder::new();
    let pair1 = (
        vec![1, 2, 1],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    pathfinder_merkle_tree
        .set(
            &storage,
            bitvec,
            PathfinderFelt::from_hex_str("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
        )
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let (_, root_id) = commit_and_persist(pathfinder_merkle_tree.clone(), &mut storage);
    let bonsai_proof = bonsai_storage
        .get_proof(&identifier, &BitVec::from_vec(vec![1, 2, 1]))
        .unwrap();
    let pathfinder_proof =
        pathfinder_merkle_tree::tree::MerkleTree::<PedersenHash, 251>::get_proof(
            root_id,
            &storage,
            &BitVec::from_vec(vec![1, 2, 1]),
        )
        .unwrap();
    assert_eq_proof(&bonsai_proof, &pathfinder_proof);
    assert_eq!(
        BonsaiStorage::<BasicId, RocksDB<BasicId>, Pedersen>::verify_proof(
            bonsai_storage.root_hash(&identifier).unwrap(),
            &BitVec::from_vec(vec![1, 2, 1]),
            pair1.1,
            &bonsai_proof
        ),
        Some(Membership::Member)
    );
}

#[test]
fn zero_not_crashing() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::<_, _, Pedersen>::new(RocksDB::new(&db, RocksDBConfig::default()), config)
            .unwrap();
    let mut id_builder = BasicIdBuilder::new();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    bonsai_storage
        .get_proof(&identifier, &BitVec::from_vec(vec![1, 2, 1]))
        .expect_err("Should error");
}
