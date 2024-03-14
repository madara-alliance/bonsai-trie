#![cfg(feature = "std")]
use bitvec::{bits, order::Msb0, vec::BitVec};
use starknet_types_core::{felt::Felt, hash::Pedersen};

use crate::{
    databases::{create_rocks_db, RocksDB, RocksDBConfig},
    id::BasicIdBuilder,
    BonsaiStorage, BonsaiStorageConfig,
};

#[test]
fn trie_height_251() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
    for i in 0..251 {
        let mut key: BitVec<u8, Msb0> = bits![u8, Msb0; 0; 251].to_bitvec();
        key.set(i, true);
        let value = Felt::from_hex("0x01").unwrap();
        bonsai_storage
            .insert(&identifier, key.as_bitslice(), &value)
            .unwrap();
    }
    let mut id_builder = BasicIdBuilder::new();
    let id = id_builder.new_id();
    bonsai_storage.commit(id).unwrap();
    bonsai_storage.root_hash(&identifier).unwrap();
}
// Test to add on Madara side to check with a tree of height 251 and see that we have same hash
// #[test]// fn test_height_251() {
//     let mut tree = super::merkle_patricia_tree::merkle_tree::MerkleTree::<PedersenHasher>::empty();
//     for i in 0..251 {
//         let mut key: BitVec<u8, Msb0> = bits![u8, Msb0; 0; 251].to_bitvec();
//         key.set(i, true);
//         let value = Felt::from_hex_be("0x01").unwrap();
//         tree.set(key.as_bitslice(), value);
//     }
//     let root_hash = tree.commit();
//     println!("root_hash: {:?}", root_hash);
// }
