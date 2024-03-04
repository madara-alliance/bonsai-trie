#![cfg(feature = "std")]
use crate::{
    databases::{create_rocks_db, HashMapDb, RocksDB, RocksDBConfig},
    id::{BasicId, BasicIdBuilder},
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
fn root_hash_similar_rocks_db() {
    let root_hash_1 = {
        let tempdir = tempfile::tempdir().unwrap();
        let db = create_rocks_db(tempdir.path()).unwrap();
        let config = BonsaiStorageConfig::default();
        let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
            BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
        let mut id_builder = BasicIdBuilder::new();
        let pair1 = (
            vec![1, 2, 1],
            Felt::from_hex("0x2acf9d2ae5a475818075672b04e317e9da3d5180fed2c5f8d6d8a5fd5a92257").unwrap(),
        );
        let bitvec = BitVec::from_vec(pair1.0.clone());
        bonsai_storage.insert(&bitvec, &pair1.1).unwrap();
        let pair2 = (
            vec![1, 2, 2],
            Felt::from_hex("0x100bd6fbfced88ded1b34bd1a55b747ce3a9fde9a914bca75571e4496b56443").unwrap(),
        );
        let bitvec = BitVec::from_vec(pair2.0.clone());
        bonsai_storage.insert(&bitvec, &pair2.1).unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        let pair3 = (
            vec![1, 2, 3],
            Felt::from_hex("0x00a038cda302fedbc4f6117648c6d3faca3cda924cb9c517b46232c6316b152f").unwrap(),
        );
        let bitvec = BitVec::from_vec(pair3.0.clone());
        bonsai_storage.insert(&bitvec, &pair3.1).unwrap();
        let pair4 = (
            vec![1, 2, 4],
            Felt::from_hex("0x02808c7d8f3745e55655ad3f51f096d0c06a41f3d76caf96bad80f9be9ced171").unwrap(),
        );
        let bitvec = BitVec::from_vec(pair4.0.clone());
        bonsai_storage.insert(&bitvec, &pair4.1).unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        bonsai_storage.root_hash().unwrap()
    };
    let root_hash_2 = {
        let tempdir = tempfile::tempdir().unwrap();
        let db = create_rocks_db(tempdir.path()).unwrap();
        let config = BonsaiStorageConfig::default();
        let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
            BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
        let mut id_builder = BasicIdBuilder::new();
        let pair1 = (
            vec![1, 2, 3],
            Felt::from_hex("0x00a038cda302fedbc4f6117648c6d3faca3cda924cb9c517b46232c6316b152f").unwrap(),
        );
        let bitvec = BitVec::from_vec(pair1.0.clone());
        bonsai_storage.insert(&bitvec, &pair1.1).unwrap();
        let pair2 = (
            vec![1, 2, 4],
            Felt::from_hex("0x02808c7d8f3745e55655ad3f51f096d0c06a41f3d76caf96bad80f9be9ced171").unwrap(),
        );
        let bitvec = BitVec::from_vec(pair2.0.clone());
        bonsai_storage.insert(&bitvec, &pair2.1).unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        bonsai_storage.root_hash().unwrap()
    };
    println!("root_hash_1: {:?}", root_hash_1.to_string());
    println!("root_hash_2: {:?}", root_hash_2.to_string());
    assert_ne!(root_hash_1, root_hash_2);
}

#[test]
fn root_hash_similar_hashmap_db() {
    let root_hash_1 = {
        let db = HashMapDb::<BasicId>::default();
        let config = BonsaiStorageConfig::default();
        let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
            BonsaiStorage::new(db, config).unwrap();
        let mut id_builder = BasicIdBuilder::new();
        let pair1 = (
            vec![1, 2, 1],
            Felt::from_hex("0x2acf9d2ae5a475818075672b04e317e9da3d5180fed2c5f8d6d8a5fd5a92257").unwrap(),
        );
        let bitvec = BitVec::from_vec(pair1.0.clone());
        bonsai_storage.insert(&bitvec, &pair1.1).unwrap();
        let pair2 = (
            vec![1, 2, 2],
            Felt::from_hex("0x100bd6fbfced88ded1b34bd1a55b747ce3a9fde9a914bca75571e4496b56443").unwrap(),
        );
        let bitvec = BitVec::from_vec(pair2.0.clone());
        bonsai_storage.insert(&bitvec, &pair2.1).unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        let pair3 = (
            vec![1, 2, 3],
            Felt::from_hex("0x00a038cda302fedbc4f6117648c6d3faca3cda924cb9c517b46232c6316b152f").unwrap(),
        );
        let bitvec = BitVec::from_vec(pair3.0.clone());
        bonsai_storage.insert(&bitvec, &pair3.1).unwrap();
        let pair4 = (
            vec![1, 2, 4],
            Felt::from_hex("0x02808c7d8f3745e55655ad3f51f096d0c06a41f3d76caf96bad80f9be9ced171").unwrap(),
        );
        let bitvec = BitVec::from_vec(pair4.0.clone());
        bonsai_storage.insert(&bitvec, &pair4.1).unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        bonsai_storage.root_hash().unwrap()
    };
    let root_hash_2 = {
        let db = HashMapDb::<BasicId>::default();
        let config = BonsaiStorageConfig::default();
        let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
            BonsaiStorage::new(db, config).unwrap();
        let mut id_builder = BasicIdBuilder::new();
        let pair1 = (
            vec![1, 2, 3],
            Felt::from_hex("0x00a038cda302fedbc4f6117648c6d3faca3cda924cb9c517b46232c6316b152f").unwrap(),
        );
        let bitvec = BitVec::from_vec(pair1.0.clone());
        bonsai_storage.insert(&bitvec, &pair1.1).unwrap();
        let pair2 = (
            vec![1, 2, 4],
            Felt::from_hex("0x02808c7d8f3745e55655ad3f51f096d0c06a41f3d76caf96bad80f9be9ced171").unwrap(),
        );
        let bitvec = BitVec::from_vec(pair2.0.clone());
        bonsai_storage.insert(&bitvec, &pair2.1).unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        bonsai_storage.root_hash().unwrap()
    };
    println!("root_hash_1: {:?}", root_hash_1.to_string());
    println!("root_hash_2: {:?}", root_hash_2.to_string());
    assert_ne!(root_hash_1, root_hash_2);
}


#[test]
fn get_changes() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
    let mut id_builder = BasicIdBuilder::new();
    let pair1 = (vec![1, 2, 1], Felt::from_hex("0x01").unwrap());
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage.insert(&bitvec, &pair1.1).unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let pair2 = (vec![1, 2, 2], Felt::from_hex("0x01").unwrap());
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage.insert(&bitvec, &pair2.1).unwrap();
    let pair1_edited_1 = (vec![1, 2, 1], Felt::from_hex("0x02").unwrap());
    let bitvec = BitVec::from_vec(pair1_edited_1.0.clone());
    bonsai_storage.insert(&bitvec, &pair1_edited_1.1).unwrap();
    let pair1_edited_2 = (vec![1, 2, 1], Felt::from_hex("0x03").unwrap());
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
