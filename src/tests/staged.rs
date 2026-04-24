#![cfg(all(feature = "std", feature = "rocksdb"))]
use crate::{
    databases::{create_rocks_db, RocksDB, RocksDBConfig},
    id::{BasicId, BasicIdBuilder},
    BitVec, BonsaiStorage, BonsaiStorageConfig,
};
use starknet_types_core::{felt::Felt, hash::Pedersen};

#[test]
fn staged_root_matches_committed_root() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 1],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let pair2 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );

    bonsai_storage
        .insert(&identifier, &BitVec::from_vec(pair1.0.clone()), &pair1.1)
        .unwrap();
    bonsai_storage
        .insert(&identifier, &BitVec::from_vec(pair2.0.clone()), &pair2.1)
        .unwrap();

    // Staged root should be computable before commit
    let staged_root = bonsai_storage.root_hash_staged(&identifier).unwrap();
    assert_ne!(staged_root, Felt::ZERO);

    // Commit and get the committed root
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let committed_root = bonsai_storage.root_hash(&identifier).unwrap();

    assert_eq!(staged_root, committed_root);
}

#[test]
fn staged_root_after_incremental_inserts() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    // Commit an initial state
    let pair1 = (
        vec![1, 2, 1],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_storage
        .insert(&identifier, &BitVec::from_vec(pair1.0.clone()), &pair1.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();

    // Insert more data without committing
    let pair2 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_storage
        .insert(&identifier, &BitVec::from_vec(pair2.0.clone()), &pair2.1)
        .unwrap();

    // Staged root reflects both pair1 (committed) and pair2 (staged)
    let staged_root = bonsai_storage.root_hash_staged(&identifier).unwrap();
    assert_ne!(staged_root, Felt::ZERO);

    // Commit and verify match
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let committed_root = bonsai_storage.root_hash(&identifier).unwrap();

    assert_eq!(staged_root, committed_root);
}

#[test]
fn staged_root_does_not_mutate_tree() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 1],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_storage
        .insert(&identifier, &BitVec::from_vec(pair1.0.clone()), &pair1.1)
        .unwrap();

    // Call staged root multiple times — should be idempotent
    let root1 = bonsai_storage.root_hash_staged(&identifier).unwrap();
    let root2 = bonsai_storage.root_hash_staged(&identifier).unwrap();
    assert_eq!(root1, root2);

    // Commit should still work after staged computations
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let committed_root = bonsai_storage.root_hash(&identifier).unwrap();
    assert_eq!(root1, committed_root);
}

#[test]
fn staged_root_on_empty_trie() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let bonsai_storage: BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);

    let root = bonsai_storage.root_hash_staged(&identifier).unwrap();
    assert_eq!(root, Felt::ZERO);
}

#[test]
fn staged_root_falls_back_to_committed_when_no_changes() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 1],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_storage
        .insert(&identifier, &BitVec::from_vec(pair1.0.clone()), &pair1.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();

    let committed_root = bonsai_storage.root_hash(&identifier).unwrap();

    // No new inserts — staged should return the committed root
    let staged_root = bonsai_storage.root_hash_staged(&identifier).unwrap();
    assert_eq!(staged_root, committed_root);
}

#[test]
fn staged_root_with_removal() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 1],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let pair2 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_storage
        .insert(&identifier, &BitVec::from_vec(pair1.0.clone()), &pair1.1)
        .unwrap();
    bonsai_storage
        .insert(&identifier, &BitVec::from_vec(pair2.0.clone()), &pair2.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();

    // Remove one key without committing
    bonsai_storage
        .remove(&identifier, &BitVec::from_vec(pair1.0.clone()))
        .unwrap();

    let staged_root = bonsai_storage.root_hash_staged(&identifier).unwrap();
    assert_ne!(staged_root, Felt::ZERO);

    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let committed_root = bonsai_storage.root_hash(&identifier).unwrap();

    assert_eq!(staged_root, committed_root);
}

#[test]
fn staged_root_with_multiple_identifiers() {
    let id_a = vec![0x01];
    let id_b = vec![0x02];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let val_a = Felt::from_hex("0xaa").unwrap();
    let val_b = Felt::from_hex("0xbb").unwrap();
    let key = vec![1, 2, 1];

    bonsai_storage
        .insert(&id_a, &BitVec::from_vec(key.clone()), &val_a)
        .unwrap();
    bonsai_storage
        .insert(&id_b, &BitVec::from_vec(key.clone()), &val_b)
        .unwrap();

    let staged_a = bonsai_storage.root_hash_staged(&id_a).unwrap();
    let staged_b = bonsai_storage.root_hash_staged(&id_b).unwrap();
    assert_ne!(staged_a, staged_b);

    bonsai_storage.commit(id_builder.new_id()).unwrap();

    let committed_a = bonsai_storage.root_hash(&id_a).unwrap();
    let committed_b = bonsai_storage.root_hash(&id_b).unwrap();

    assert_eq!(staged_a, committed_a);
    assert_eq!(staged_b, committed_b);
}
