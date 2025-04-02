#![cfg(all(feature = "std", feature = "rocksdb"))]
use crate::{
    databases::{create_rocks_db, RocksDB, RocksDBConfig},
    id::BasicIdBuilder,
    BitVec, BonsaiStorage, BonsaiStorageConfig, BonsaiTrieHash,
};
use starknet_types_core::{felt::Felt, hash::Pedersen};

#[test]
fn basics() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 1],
        &Felt::from_hex("0x16342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();
    let _root_hash1 = bonsai_storage.root_hash(&identifier).unwrap();

    let id2 = id_builder.new_id();
    let pair2 = (
        vec![1, 2, 2],
        &Felt::from_hex("0x66342762FDD54D3c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, pair2.1)
        .unwrap();
    bonsai_storage.commit(id2).unwrap();
    let _root_hash2 = bonsai_storage.root_hash(&identifier).unwrap();

    // TODO: uncomment and fix this or remove it
    //
    // let id3 = id_builder.new_id();
    // let bitvec = BitVec::from_vec(pair1.0);
    // bonsai_storage.remove(&identifier, &bitvec).unwrap();
    // bonsai_storage.commit(id3).unwrap();

    // bonsai_storage.revert_to(id2).unwrap();
    // let revert_root_hash2 = bonsai_storage.root_hash(&identifier).unwrap();

    // bonsai_storage.revert_to(id1).unwrap();
    // let revert_root_hash1 = bonsai_storage.root_hash(&identifier).unwrap();

    // assert_eq!(root_hash2, revert_root_hash2);
    // assert_eq!(root_hash1, revert_root_hash1);
}

#[test]
fn revert_across_value_change() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let key = vec![1, 2, 1];

    let pair1 = (
        key.clone(),
        &Felt::from_hex("0x00000000000000000000000000000000000001").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();
    let root_hash1 = bonsai_storage.root_hash(&identifier).unwrap();

    let id2 = id_builder.new_id();
    let pair2 = (
        key,
        &Felt::from_hex("0x000000000000000000000000000000000011").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, pair2.1)
        .unwrap();
    bonsai_storage.commit(id2).unwrap();

    bonsai_storage.revert_to(id1, id2).unwrap();
    assert_eq!(root_hash1, bonsai_storage.root_hash(&identifier).unwrap());
}

#[test]
fn unrecorded_revert() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FDD54D3c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();

    let uncommited_id = id_builder.new_id();
    bonsai_storage.revert_to(uncommited_id, id1).unwrap_err();
}

#[test]
fn in_place_revert() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (vec![1, 2, 3], &BonsaiTrieHash::default());
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();
    let root_hash1 = bonsai_storage.root_hash(&identifier).unwrap();

    bonsai_storage.revert_to(id1, id1).unwrap();
    assert_eq!(root_hash1, bonsai_storage.root_hash(&identifier).unwrap());
}

#[test]
fn truncated_revert() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 1],
        &Felt::from_hex("0x16342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();
    let root_hash1 = bonsai_storage.root_hash(&identifier).unwrap();

    let id2 = id_builder.new_id();
    let pair2 = (
        vec![1, 2, 2],
        &Felt::from_hex("0x66342762FDD54D3c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, pair2.1)
        .unwrap();
    bonsai_storage.commit(id2).unwrap();

    bonsai_storage.revert_to(id1, id2).unwrap();
    let revert_root_hash1 = bonsai_storage.root_hash(&identifier).unwrap();
    bonsai_storage.revert_to(id2, id1).unwrap_err();

    assert_eq!(root_hash1, revert_root_hash1);
}

#[test]
fn double_revert() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 1],
        &Felt::from_hex("0x16342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();
    let root_hash1 = bonsai_storage.root_hash(&identifier).unwrap();

    let id2 = id_builder.new_id();
    let pair2 = (
        vec![1, 2, 2],
        &Felt::from_hex("0x66342762FDD54D3c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, pair2.1)
        .unwrap();
    bonsai_storage.commit(id2).unwrap();

    bonsai_storage.revert_to(id1, id2).unwrap();
    let revert1 = bonsai_storage.root_hash(&identifier).unwrap();
    bonsai_storage.revert_to(id1, id1).unwrap();
    let revert2 = bonsai_storage.root_hash(&identifier).unwrap();

    assert_eq!(root_hash1, revert1);
    assert_eq!(revert1, revert2);
}

#[test]
fn remove_and_reinsert() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FDD54D3c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.remove(&identifier, &bitvec).unwrap();
    bonsai_storage.commit(id1).unwrap();
    let root_hash1 = bonsai_storage.root_hash(&identifier).unwrap();
    let id2 = id_builder.new_id();
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id2).unwrap();

    bonsai_storage.revert_to(id1, id2).unwrap();
    assert_eq!(root_hash1, bonsai_storage.root_hash(&identifier).unwrap());
}

/// Tests a case where we call revert_to with a `current_id` that is behind the latest
#[test]
fn revert_old_new_id() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 1],
        &Felt::from_hex("0x16342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();
    let root_hash1 = bonsai_storage.root_hash(&identifier).unwrap();

    let id2 = id_builder.new_id();
    let pair2 = (
        vec![1, 2, 2],
        &Felt::from_hex("0x66342762FDD54D3c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, pair2.1)
        .unwrap();
    bonsai_storage.commit(id2).unwrap();
    let root_hash2 = bonsai_storage.root_hash(&identifier).unwrap();

    bonsai_storage.revert_to(id1, id1).unwrap_err();
    let revert = bonsai_storage.root_hash(&identifier).unwrap();

    assert_eq!(root_hash2, revert);
}
