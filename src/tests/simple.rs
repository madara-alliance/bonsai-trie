use crate::{
    databases::{create_rocks_db, RocksDB, RocksDBConfig},
    id::BasicIdBuilder,
    BonsaiStorage, BonsaiStorageConfig,
};
use bitvec::vec::BitVec;
use mp_felt::Felt;

#[test]
fn basics() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
    let mut id_builder = BasicIdBuilder::new();
    let pair1 = (
        vec![1, 2, 1],
        Felt::from_hex_be("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage.insert(&bitvec, &pair1.1).unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let pair2 = (
        vec![1, 2, 2],
        Felt::from_hex_be("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage.insert(&bitvec, &pair2.1).unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let pair3 = (
        vec![1, 2, 3],
        Felt::from_hex_be("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair3.0.clone());
    bonsai_storage.insert(&bitvec, &pair3.1).unwrap();
    println!(
        "get: {:?}",
        bonsai_storage.get(&BitVec::from_vec(vec![1, 2, 1]))
    );
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    println!(
        "get: {:?}",
        bonsai_storage.get(&BitVec::from_vec(vec![1, 2, 2]))
    );
    println!(
        "get: {:?}",
        bonsai_storage.get(&BitVec::from_vec(vec![1, 2, 3]))
    );
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
