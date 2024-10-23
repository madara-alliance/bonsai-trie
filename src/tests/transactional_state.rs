#![cfg(all(feature = "std", feature = "rocksdb"))]
use crate::{
    databases::{create_rocks_db, RocksDB, RocksDBConfig},
    id::BasicIdBuilder,
    BitVec, BonsaiStorage, BonsaiStorageConfig,
};
use log::LevelFilter;
use starknet_types_core::{felt::Felt, hash::Pedersen};

#[test]
fn basics() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24).unwrap();
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();

    let pair2 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let id2 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair2.1)
        .unwrap();
    bonsai_storage.commit(id2).unwrap();

    let bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
        .get_transactional_state(id1, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    assert_eq!(
        bonsai_at_txn.get(&identifier, &bitvec).unwrap().unwrap(),
        pair1.1
    );
    let bitvec = BitVec::from_vec(pair2.0.clone());
    assert!(bonsai_at_txn.get(&identifier, &bitvec).unwrap().is_none());
}

#[test]
fn test_thread() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage = BonsaiStorage::new(
        RocksDB::new(&db, RocksDBConfig::default()),
        config.clone(),
        24,
    )
    .unwrap();
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();

    std::thread::scope(|s| {
        s.spawn(|| {
            let bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
                .get_transactional_state(id1, bonsai_storage.get_config())
                .unwrap()
                .unwrap();
            let bitvec = BitVec::from_vec(pair1.0.clone());
            assert_eq!(
                bonsai_at_txn.get(&identifier, &bitvec).unwrap().unwrap(),
                pair1.1
            );
        });

        s.spawn(|| {
            let bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
                .get_transactional_state(id1, bonsai_storage.get_config())
                .unwrap()
                .unwrap();
            let bitvec = BitVec::from_vec(pair1.0.clone());
            assert_eq!(
                bonsai_at_txn.get(&identifier, &bitvec).unwrap().unwrap(),
                pair1.1
            );
        });
    });

    bonsai_storage
        .get(&identifier, &BitVec::from_vec(vec![1, 2, 2]))
        .unwrap();
    let pair2 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_storage
        .insert(&identifier, &BitVec::from_vec(pair2.0.clone()), &pair2.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
}

#[test]
fn remove() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24).unwrap();
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();

    let id2 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage.remove(&identifier, &bitvec).unwrap();
    bonsai_storage.commit(id2).unwrap();
    bonsai_storage.dump_database();

    let bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
        .get_transactional_state(id1, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    assert_eq!(
        bonsai_at_txn.get(&identifier, &bitvec).unwrap().unwrap(),
        pair1.1
    );

    let bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
        .get_transactional_state(id2, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    assert!(bonsai_at_txn.get(&identifier, &bitvec).unwrap().is_none());
}

#[test]
fn merge() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24).unwrap();
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FDD5D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();
    let mut bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
        .get_transactional_state(id1, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();
    let pair2 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_at_txn
        .insert(&identifier, &BitVec::from_vec(pair2.0.clone()), &pair2.1)
        .unwrap();
    bonsai_at_txn
        .transactional_commit(id_builder.new_id())
        .unwrap();
    bonsai_storage.merge(bonsai_at_txn).unwrap();
    assert_eq!(
        bonsai_storage
            .get(&identifier, &BitVec::from_vec(vec![1, 2, 3]))
            .unwrap(),
        Some(pair2.1)
    );
}

#[test]
fn merge_with_uncommitted_insert() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24).unwrap();
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        BitVec::from_vec(vec![1, 2, 2]),
        Felt::from_hex("0x66342762FDD5D033c195fec3ce2568b62052e").unwrap(),
    );
    let pair2 = (
        BitVec::from_vec(vec![1, 2, 3]),
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );

    let id1 = id_builder.new_id();
    bonsai_storage
        .insert(&identifier, &pair1.0, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();

    let mut bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
        .get_transactional_state(id1, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();
    bonsai_at_txn
        .insert(&identifier, &pair2.0, &pair2.1)
        .unwrap();

    bonsai_storage.merge(bonsai_at_txn).unwrap();

    // commit after merge
    let revert_id = id_builder.new_id();
    bonsai_storage.commit(revert_id).unwrap();

    // overwrite pair2
    bonsai_storage
        .insert(
            &identifier,
            &pair2.0,
            &Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052F").unwrap(),
        )
        .unwrap();

    println!("{:?}", bonsai_storage);

    // revert to commit
    bonsai_storage.revert_to(revert_id).unwrap();

    assert_eq!(
        bonsai_storage.get(&identifier, &pair2.0).unwrap(),
        Some(pair2.1)
    );
}

#[test]
fn merge_with_uncommitted_remove() {
    let _ = env_logger::builder().is_test(true).try_init();
    log::set_max_level(LevelFilter::Trace);

    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24).unwrap();
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        BitVec::from_vec(vec![1, 2, 2]),
        Felt::from_hex("0x66342762FDD5D033c195fec3ce2568b62052e").unwrap(),
    );
    let pair2 = (
        BitVec::from_vec(vec![1, 2, 3]),
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );

    let id1 = id_builder.new_id();
    bonsai_storage
        .insert(&identifier, &pair1.0, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();

    let mut bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
        .get_transactional_state(id1, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();
    bonsai_at_txn
        .insert(&identifier, &pair2.0, &pair2.1)
        .unwrap();
    bonsai_at_txn
        .transactional_commit(id_builder.new_id())
        .unwrap();

    // remove pair2 but don't commit in transational state
    bonsai_at_txn.remove(&identifier, &pair2.0).unwrap();
    assert!(!bonsai_at_txn.contains(&identifier, &pair2.0).unwrap());

    let merge = bonsai_storage.merge(bonsai_at_txn);
    match merge {
        Ok(_) => println!("merge succeeded"),
        Err(e) => {
            println!("merge failed");
            panic!("{}", e);
        }
    };

    // commit after merge
    bonsai_storage.commit(id_builder.new_id()).unwrap();

    assert!(!bonsai_storage.contains(&identifier, &pair2.0).unwrap());
}

#[test]
fn transactional_state_after_uncommitted() {
    let _ = env_logger::builder().is_test(true).try_init();
    log::set_max_level(LevelFilter::Trace);

    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        BitVec::from_vec(vec![1, 2, 2]),
        Felt::from_hex("0x66342762FDD5D033c195fec3ce2568b62052e").unwrap(),
    );
    let pair2 = (
        BitVec::from_vec(vec![1, 2, 3]),
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );

    let id1 = id_builder.new_id();
    bonsai_storage
        .insert(&identifier, &pair1.0, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();

    // make a change to original tree but don't commit it
    bonsai_storage
        .insert(&identifier, &pair2.0, &pair2.1)
        .unwrap();

    // create a transactional state after the uncommitted change
    let bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
        .get_transactional_state(id1, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();

    // uncommitted changes, done after the transactional state was created,
    // are not included in the transactional state
    let contains = bonsai_at_txn.contains(&identifier, &pair2.0).unwrap();
    assert!(!contains);
}

#[test]
fn merge_override() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FDD5D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();
    let mut bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
        .get_transactional_state(id1, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();
    let pair2 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_at_txn
        .insert(&identifier, &BitVec::from_vec(pair2.0.clone()), &pair2.1)
        .unwrap();
    bonsai_at_txn
        .transactional_commit(id_builder.new_id())
        .unwrap();
    bonsai_storage.merge(bonsai_at_txn).unwrap();
    assert_eq!(
        bonsai_storage
            .get(&identifier, &BitVec::from_vec(vec![1, 2, 2]))
            .unwrap(),
        Some(pair2.1)
    );
}

#[test]
fn merge_remove() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FDD5D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();
    let mut bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
        .get_transactional_state(id1, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();
    bonsai_at_txn
        .remove(&identifier, &BitVec::from_vec(pair1.0.clone()))
        .unwrap();
    bonsai_at_txn
        .transactional_commit(id_builder.new_id())
        .unwrap();
    bonsai_storage.merge(bonsai_at_txn).unwrap();
    assert_eq!(
        bonsai_storage
            .get(&identifier, &BitVec::from_vec(pair1.0))
            .unwrap(),
        None
    );
}

#[test]
fn merge_txn_revert() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FDD5D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();
    let root_hash1 = bonsai_storage.root_hash(&identifier).unwrap();

    let mut bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
        .get_transactional_state(id1, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();
    bonsai_at_txn
        .remove(&identifier, &BitVec::from_vec(pair1.0.clone()))
        .unwrap();
    let id2 = id_builder.new_id();
    bonsai_at_txn.transactional_commit(id2).unwrap();
    let root_hash2 = bonsai_at_txn.root_hash(&identifier).unwrap();

    let pair2 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_at_txn
        .insert(&identifier, &BitVec::from_vec(pair2.0.clone()), &pair2.1)
        .unwrap();
    let id3 = id_builder.new_id();
    bonsai_at_txn.transactional_commit(id3).unwrap();

    bonsai_at_txn.revert_to(id2).unwrap();
    let revert_hash2 = bonsai_at_txn.root_hash(&identifier).unwrap();
    bonsai_at_txn.revert_to(id1).unwrap();
    let revert_hash1 = bonsai_at_txn.root_hash(&identifier).unwrap();

    assert_eq!(root_hash2, revert_hash2);
    assert_eq!(root_hash1, revert_hash1);

    bonsai_storage.merge(bonsai_at_txn).unwrap();
    assert_eq!(
        bonsai_storage
            .get(&identifier, &BitVec::from_vec(pair1.0))
            .unwrap(),
        Some(pair1.1)
    );
}

#[test]
fn merge_invalid() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FDD5D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();

    let mut bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
        .get_transactional_state(id1, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();
    bonsai_at_txn
        .remove(&identifier, &BitVec::from_vec(pair1.0.clone()))
        .unwrap();
    let id2 = id_builder.new_id();
    bonsai_at_txn.transactional_commit(id2).unwrap();

    let pair2 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_storage
        .insert(&identifier, &BitVec::from_vec(pair2.0.clone()), &pair2.1)
        .unwrap();
    let id3 = id_builder.new_id();
    bonsai_storage.commit(id3).unwrap();

    bonsai_storage.merge(bonsai_at_txn).unwrap_err();
}

#[test]
fn many_snapshots() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig {
        snapshot_interval: 1,
        ..Default::default()
    };
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config, 24);
    let mut id_builder = BasicIdBuilder::new();

    let pair1 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let id1 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id1).unwrap();

    let pair2 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let id2 = id_builder.new_id();
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair2.1)
        .unwrap();
    bonsai_storage.commit(id2).unwrap();

    bonsai_storage
        .get_transactional_state(id1, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();
    bonsai_storage
        .get_transactional_state(id2, BonsaiStorageConfig::default())
        .unwrap()
        .unwrap();
}
