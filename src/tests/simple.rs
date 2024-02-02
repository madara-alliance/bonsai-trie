#![cfg(feature = "std")]
use crate::{
    databases::{create_rocks_db, RocksDB, RocksDBConfig},
    id::BasicIdBuilder,
    BonsaiStorage, BonsaiStorageConfig, Change,
};
use bitvec::vec::BitVec;
use starknet_types_core::{felt::Felt, hash::Pedersen};

#[test]
fn basics() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
    let mut id_builder = BasicIdBuilder::new();
    let pair1 = (
        vec![1, 2, 1],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage.insert(&bitvec, &pair1.1).unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let pair2 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage.insert(&bitvec, &pair2.1).unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let pair3 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair3.0.clone());
    bonsai_storage.insert(&bitvec, &pair3.1).unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let bitvec = BitVec::from_vec(vec![1, 2, 1]);
    bonsai_storage.remove(&bitvec).unwrap();
    assert_eq!(
        bonsai_storage
            .get(&BitVec::from_vec(vec![1, 2, 1]))
            .unwrap(),
        None
    );
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    assert_eq!(
        bonsai_storage
            .get(&BitVec::from_vec(vec![1, 2, 1]))
            .unwrap(),
        None
    );
}

#[test]
fn get_changes() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
    let mut id_builder = BasicIdBuilder::new();
    let pair1 = (
        vec![1, 2, 1],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage.insert(&bitvec, &pair1.1).unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let pair2 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage.insert(&bitvec, &pair2.1).unwrap();
    let pair1_edited_1 = (
        vec![1, 2, 1],
        Felt::from_hex("0x66342762FDD54D033c195fec1ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair1_edited_1.0.clone());
    bonsai_storage.insert(&bitvec, &pair1_edited_1.1).unwrap();
    let pair1_edited_2 = (
        vec![1, 2, 1],
        Felt::from_hex("0x66342762FDD54D033c195fec1ce2568b62051e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair1_edited_2.0.clone());
    bonsai_storage.insert(&bitvec, &pair1_edited_2.1).unwrap();
    let id = id_builder.new_id();
    bonsai_storage.commit(id).unwrap();
    let changes = bonsai_storage.get_changes(id).unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(
        changes.get(&BitVec::from_vec(pair1.0)).unwrap(),
        &Change {
            old_value: Some(pair1.1),
            new_value: Some(pair1_edited_2.1),
        }
    );
    assert_eq!(
        changes.get(&BitVec::from_vec(pair2.0)).unwrap(),
        &Change {
            old_value: None,
            new_value: Some(pair2.1),
        }
    );
}
