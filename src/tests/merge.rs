#![allow(clippy::type_complexity)]
#![cfg(all(feature = "std", feature = "rocksdb"))]

use crate::{
    databases::{create_rocks_db, RocksDB, RocksDBConfig, RocksDBTransaction},
    id::{BasicId, BasicIdBuilder},
    BitVec, BonsaiStorage, BonsaiStorageConfig,
};
use once_cell::sync::Lazy;
use rocksdb::OptimisticTransactionDB;
use starknet_types_core::{felt::Felt, hash::Pedersen};

static PAIR1: Lazy<(BitVec, Felt)> = Lazy::new(|| {
    (
        BitVec::from_vec(vec![1, 2, 2]),
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    )
});

static PAIR2: Lazy<(BitVec, Felt)> = Lazy::new(|| {
    (
        BitVec::from_vec(vec![1, 2, 3]),
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    )
});

static PAIR3: Lazy<(BitVec, Felt)> = Lazy::new(|| {
    (
        BitVec::from_vec(vec![1, 2, 4]),
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    )
});

/// Initializes a test environment for the BonsaiStorage data structure.
///
/// # Arguments
///
/// * `db` - An instance of the `OptimisticTransactionDB` struct.
///
/// # Returns
///
/// A tuple containing the following elements:
/// * `identifier` - A vector of bytes.
/// * `bonsai_storage` - An instance of `BonsaiStorage` with a `RocksDB`
///   backend.
/// * `bonsai_at_txn` - An instance of `BonsaiStorage` representing the
///   transactional state of `bonsai_storage` at `start_id`.
/// * `id_builder` - An instance of `BasicIdBuilder`.
/// * `start_id` - A `BasicId` representing the commit ID of the changes made in
///   `bonsai_storage`.
fn init_test(
    db: &OptimisticTransactionDB,
) -> (
    Vec<u8>,
    BonsaiStorage<BasicId, RocksDB<'_, BasicId>, Pedersen>,
    BonsaiStorage<BasicId, RocksDBTransaction<'_>, Pedersen>,
    BasicIdBuilder,
    BasicId,
) {
    let identifier = vec![];

    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::new(RocksDB::new(db, RocksDBConfig::default()), config, 24)
            .expect("Failed to create BonsaiStorage");

    let mut id_builder = BasicIdBuilder::new();

    bonsai_storage
        .insert(&identifier, &PAIR1.0, &PAIR1.1)
        .expect("Failed to insert key-value pair");

    let start_id = id_builder.new_id();
    bonsai_storage
        .commit(start_id)
        .expect("Failed to commit changes");

    let bonsai_at_txn = bonsai_storage
        .get_transactional_state(start_id, BonsaiStorageConfig::default())
        .expect("Failed to get transactional state")
        .expect("Transactional state not found");

    (
        identifier,
        bonsai_storage,
        bonsai_at_txn,
        id_builder,
        start_id,
    )
}

#[test]
fn merge_before_simple() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, _) = init_test(&db);

    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();
    bonsai_storage.merge(bonsai_at_txn).unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();

    assert_eq!(
        bonsai_storage.get(&identifier, &PAIR2.0).unwrap(),
        Some(PAIR2.1)
    );
}

#[test]
fn merge_before_simple_remove() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, start_id) =
        init_test(&db);

    bonsai_at_txn.remove(&identifier, &PAIR1.0).unwrap();
    bonsai_storage.merge(bonsai_at_txn).unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();

    assert!(!bonsai_storage.contains(&identifier, &PAIR1.0).unwrap());

    bonsai_storage.revert_to(start_id).unwrap();

    assert_eq!(
        bonsai_storage.get(&identifier, &PAIR1.0).unwrap(),
        Some(PAIR1.1)
    );
}

#[test]
fn merge_tx_commit_simple_remove() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, start_id) =
        init_test(&db);

    bonsai_at_txn.remove(&identifier, &PAIR1.0).unwrap();
    bonsai_at_txn
        .transactional_commit(id_builder.new_id())
        .unwrap();

    bonsai_storage.merge(bonsai_at_txn).unwrap();

    assert!(!bonsai_storage.contains(&identifier, &PAIR1.0).unwrap());

    bonsai_storage.revert_to(start_id).unwrap();

    assert_eq!(
        bonsai_storage.get(&identifier, &PAIR1.0).unwrap(),
        Some(PAIR1.1)
    );
}

#[test]
fn merge_before_simple_revert_to() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, start_id) =
        init_test(&db);

    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();
    bonsai_storage.merge(bonsai_at_txn).unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    bonsai_storage.revert_to(start_id).unwrap();

    assert!(bonsai_storage.get(&identifier, &PAIR2.0).unwrap().is_none());
}

#[test]
fn merge_transactional_commit_in_txn_before() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, start_id) =
        init_test(&db);

    let id2 = id_builder.new_id();
    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();
    bonsai_at_txn.transactional_commit(id2).unwrap();

    let id3 = id_builder.new_id();
    bonsai_at_txn
        .insert(&identifier, &PAIR3.0, &PAIR3.1)
        .unwrap();
    bonsai_storage.merge(bonsai_at_txn).unwrap();
    bonsai_storage.commit(id3).unwrap();

    assert_eq!(
        bonsai_storage.get(&identifier, &PAIR2.0).unwrap(),
        Some(PAIR2.1)
    );
    assert_eq!(
        bonsai_storage.get(&identifier, &PAIR3.0).unwrap(),
        Some(PAIR3.1)
    );
    bonsai_storage.revert_to(id2).unwrap();
    assert_eq!(
        bonsai_storage.get(&identifier, &PAIR2.0).unwrap(),
        Some(PAIR2.1)
    );
    assert!(bonsai_storage.get(&identifier, &PAIR3.0).unwrap().is_none());
    bonsai_storage.revert_to(start_id).unwrap();
    assert!(bonsai_storage.get(&identifier, &PAIR2.0).unwrap().is_none());
    assert!(bonsai_storage.get(&identifier, &PAIR3.0).unwrap().is_none());
}

#[test]
fn merge_transactional_commit_in_txn_before_existing_key() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, start_id) =
        init_test(&db);

    bonsai_at_txn.remove(&identifier, &PAIR1.0).unwrap();

    let id2 = id_builder.new_id();
    bonsai_at_txn.transactional_commit(id2).unwrap();
    bonsai_storage.merge(bonsai_at_txn).unwrap();
    bonsai_storage.revert_to(id2).unwrap();

    assert!(bonsai_storage.get(&identifier, &PAIR1.0).unwrap().is_none());

    bonsai_storage.revert_to(start_id).unwrap();

    assert_eq!(
        bonsai_storage.get(&identifier, &PAIR1.0.clone()).unwrap(),
        Some(PAIR1.1)
    );
}

#[test]
fn merge_get_uncommitted() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, _, _) = init_test(&db);

    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();
    bonsai_storage.merge(bonsai_at_txn).unwrap();

    assert_eq!(
        bonsai_storage.get(&identifier, &PAIR2.0).unwrap(),
        Some(PAIR2.1)
    );
}

#[test]
fn merge_conflict_commited_vs_commited() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, _) = init_test(&db);

    // insert a key in the transactional state
    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();
    let id = id_builder.new_id();
    bonsai_at_txn.transactional_commit(id).unwrap();

    // insert same key with a different value in the bonsai_storage
    bonsai_storage
        .insert(&identifier, &PAIR2.0, &PAIR3.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();

    match bonsai_storage.merge(bonsai_at_txn) {
        Ok(_) => panic!("Expected merge conflict error"),
        Err(err) => assert_eq!(
            err.to_string(),
            "Merge error: Transaction created_at BasicId(0) is lower than the last recorded id"
        ),
    }
}

#[test]
fn merge_conflict_commited_vs_commited_change_order() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, _) = init_test(&db);

    // insert same key with a different value in the bonsai_storage
    bonsai_storage
        .insert(&identifier, &PAIR2.0, &PAIR3.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();

    // insert a key in the transactional state
    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();
    bonsai_at_txn
        .transactional_commit(id_builder.new_id())
        .unwrap();

    match bonsai_storage.merge(bonsai_at_txn) {
        Ok(_) => panic!("Expected merge conflict error"),
        Err(err) => assert_eq!(
            err.to_string(),
            "Merge error: Transaction created_at BasicId(0) is lower than the last recorded id"
        ),
    }
}

#[test]
fn merge_conflict_commited_vs_commited_and_noncommited() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, _) = init_test(&db);

    // insert a key in the transactional state
    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();
    let id = id_builder.new_id();
    bonsai_at_txn.transactional_commit(id).unwrap();
    bonsai_at_txn
        .insert(&identifier, &PAIR3.0, &PAIR3.1)
        .unwrap();

    // insert same key with a different value in the bonsai_storage
    bonsai_storage
        .insert(&identifier, &PAIR2.0, &PAIR3.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();

    match bonsai_storage.merge(bonsai_at_txn) {
        Ok(_) => panic!("Expected merge conflict error"),
        Err(err) => assert_eq!(
            err.to_string(),
            "Merge error: Transaction created_at BasicId(0) is lower than the last recorded id"
        ),
    }
}

#[test]
fn merge_conflict_commited_and_noncommited_vs_commited() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, _) = init_test(&db);

    // insert a key in the transactional state
    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();
    let id = id_builder.new_id();
    bonsai_at_txn.transactional_commit(id).unwrap();

    // insert same key with a different value in the bonsai_storage
    bonsai_storage
        .insert(&identifier, &PAIR2.0, &PAIR3.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    bonsai_storage.remove(&identifier, &PAIR2.0).unwrap();
    // .insert(&identifier, &PAIR3.0, &PAIR3.1)
    // .unwrap();

    match bonsai_storage.merge(bonsai_at_txn) {
        Ok(_) => panic!("Expected merge conflict error"),
        Err(err) => assert_eq!(
            err.to_string(),
            "Merge error: Transaction created_at BasicId(0) is lower than the last recorded id"
        ),
    }
}

#[test]
fn merge_conflict_commited_and_noncommited_vs_noncommited() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, _) = init_test(&db);

    // insert a key in the transactional state
    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();

    // insert same key with a different value in the bonsai_storage
    bonsai_storage
        .insert(&identifier, &PAIR2.0, &PAIR3.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    bonsai_storage.remove(&identifier, &PAIR2.0).unwrap();
    // .insert(&identifier, &PAIR3.0, &PAIR3.1)
    // .unwrap();

    match bonsai_storage.merge(bonsai_at_txn) {
        Ok(_) => panic!("Expected merge conflict error"),
        Err(err) => assert_eq!(
            err.to_string(),
            "Merge error: Transaction created_at BasicId(0) is lower than the last recorded id"
        ),
    }
}

#[test]
fn merge_conflict_commited_vs_noncommited() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, _) = init_test(&db);

    // insert a key in the transactional state
    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();

    // insert same key with a different value in the bonsai_storage
    bonsai_storage
        .insert(&identifier, &PAIR2.0, &PAIR3.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();

    match bonsai_storage.merge(bonsai_at_txn) {
        Ok(_) => panic!("Expected merge conflict error"),
        Err(err) => assert_eq!(
            err.to_string(),
            "Merge error: Transaction created_at BasicId(0) is lower than the last recorded id"
        ),
    }
}

#[test]
fn merge_conflict_noncommited_vs_commited() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, _) = init_test(&db);

    // insert same key with a different value in the bonsai_storage
    bonsai_storage
        .insert(&identifier, &PAIR2.0, &PAIR3.1)
        .unwrap();

    // insert a key in the transactional state
    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();
    bonsai_at_txn
        .transactional_commit(id_builder.new_id())
        .unwrap();

    bonsai_storage.merge(bonsai_at_txn).unwrap();

    // check that changes in the transactional state overwrite the ones in the
    // storage
    let get = bonsai_storage.get(&identifier, &PAIR2.0).unwrap();
    assert_eq!(get, Some(PAIR2.1));
}

#[test]
fn merge_conflict_noncommited_vs_noncommited() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, _, _) = init_test(&db);

    // insert same key with a different value in the bonsai_storage
    bonsai_storage
        .insert(&identifier, &PAIR2.0, &PAIR3.1)
        .unwrap();

    // insert a key in the transactional state
    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();

    bonsai_storage.merge(bonsai_at_txn).unwrap();

    // check that changes in the transactional state overwrite the ones in the
    // storage
    let get = bonsai_storage.get(&identifier, &PAIR2.0).unwrap();
    assert_eq!(get, Some(PAIR2.1));
}

#[test]
fn merge_conflict_noncommited_vs_commited_noncommited() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, _) = init_test(&db);

    // insert same key with a different value in the bonsai_storage
    bonsai_storage
        .insert(&identifier, &PAIR2.0, &PAIR3.1)
        .unwrap();

    // insert a key in the transactional state
    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();
    bonsai_at_txn
        .transactional_commit(id_builder.new_id())
        .unwrap();
    bonsai_at_txn
        .insert(&identifier, &PAIR3.0, &PAIR3.1)
        .unwrap();

    bonsai_storage.merge(bonsai_at_txn).unwrap();

    // change in the transactional state overwrites any noncommited changes in
    // the storage
    let get = bonsai_storage.get(&identifier, &PAIR2.0).unwrap();
    assert_eq!(get, Some(PAIR2.1));
    let get = bonsai_storage.get(&identifier, &PAIR3.0).unwrap();
    assert_eq!(get, Some(PAIR2.1));
}

#[test]
fn merge_conflict_commited_noncommited_vs_commited_noncommited() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, _) = init_test(&db);

    // insert same key with a different value in the bonsai_storage
    bonsai_storage
        .insert(&identifier, &PAIR2.0, &PAIR3.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    bonsai_storage
        .insert(&identifier, &PAIR3.0, &PAIR2.1)
        .unwrap();

    // insert a key in the transactional state
    bonsai_at_txn
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();
    bonsai_at_txn
        .transactional_commit(id_builder.new_id())
        .unwrap();
    bonsai_at_txn
        .insert(&identifier, &PAIR3.0, &PAIR3.1)
        .unwrap();

    match bonsai_storage.merge(bonsai_at_txn) {
        Ok(_) => {
            panic!("Expected merge conflict error")
        }
        Err(err) => assert_eq!(
            err.to_string(),
            "Merge error: Transaction created_at BasicId(0) is lower than the last recorded id"
        ),
    }
}

#[test]
fn merge_nonconflict_commited_vs_commited() {
    let db = create_rocks_db(tempfile::tempdir().unwrap().path()).unwrap();
    let (identifier, mut bonsai_storage, mut bonsai_at_txn, mut id_builder, _) = init_test(&db);

    bonsai_storage
        .insert(&identifier, &PAIR2.0, &PAIR2.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();

    bonsai_at_txn
        .insert(&identifier, &PAIR3.0, &PAIR3.1)
        .unwrap();
    bonsai_at_txn
        .transactional_commit(id_builder.new_id())
        .unwrap();

    match bonsai_storage.merge(bonsai_at_txn) {
        Ok(_) => {
            panic!("Expected merge conflict error")
        }
        Err(err) => assert_eq!(
            err.to_string(),
            "Merge error: Transaction created_at BasicId(0) is lower than the last recorded id"
        ),
    }
}
