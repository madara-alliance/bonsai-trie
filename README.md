# bonsai-trie

![example workflow](https://github.com/massalabs/bonsai-trie/actions/workflows/check_lint.yml/badge.svg) ![example workflow](https://github.com/massalabs/bonsai-trie/actions/workflows/test.yml/badge.svg) [![codecov](https://codecov.io/gh/massalabs/bonsai-trie/graph/badge.svg?token=598URC32TV)](https://codecov.io/gh/massalabs/bonsai-trie)


This crate provides a storage implementation based on the Bonsai Storage implemented by [Besu](https://hackmd.io/@kt2am/BktBblIL3).
It is a key/value storage that uses a Madara Merkle Trie to store the data.

## Build:

```
cargo build
```

## Doc and example:
```
cargo doc --open
```

## Example:
```rust
use bonsai_trie::{
    databases::{RocksDB, create_rocks_db, RocksDBConfig},
    BonsaiStorageError,
    id::{BasicIdBuilder, BasicId},
    BonsaiStorage, BonsaiStorageConfig, BonsaiTrieHash,
};
use mp_felt::Felt252Wrapper;
use bitvec::prelude::*;
fn main() {
    let db = create_rocks_db("./rocksdb").unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage = BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
    let mut id_builder = BasicIdBuilder::new();
    let pair1 = (vec![1, 2, 1], Felt252Wrapper::from_hex_be("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap());
    let bitvec_1 = BitVec::from_vec(pair1.0.clone());
    bonsai_storage.insert(&bitvec_1, &pair1.1).unwrap();
    let pair2 = (vec![1, 2, 2], Felt252Wrapper::from_hex_be("0x66342762FD54D033c195fec3ce2568b62052e").unwrap());
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage.insert(&bitvec, &pair2.1).unwrap();
    let id1 = id_builder.new_id();
    bonsai_storage.commit(id1);
    let pair3 = (vec![1, 2, 2], Felt252Wrapper::from_hex_be("0x664D033c195fec3ce2568b62052e").unwrap());
    let bitvec = BitVec::from_vec(pair3.0.clone());
    bonsai_storage.insert(&bitvec, &pair3.1).unwrap();
    let revert_to_id = id_builder.new_id();
    bonsai_storage.commit(revert_to_id);
    bonsai_storage.remove(&bitvec).unwrap();
    bonsai_storage.commit(id_builder.new_id());
    println!("root: {:#?}", bonsai_storage.root_hash());
    println!(
        "value: {:#?}",
        bonsai_storage.get(&bitvec_1).unwrap()
    );
    bonsai_storage.revert_to(revert_to_id).unwrap();
    println!("root: {:#?}", bonsai_storage.root_hash());
    println!("value: {:#?}", bonsai_storage.get(&bitvec).unwrap());
    std::thread::scope(|s| {
        s.spawn(|| {
            let bonsai_at_txn = bonsai_storage
                .get_transactional_state(id1, bonsai_storage.get_config())
                .unwrap()
                .unwrap();
            let bitvec = BitVec::from_vec(pair1.0.clone());
            assert_eq!(bonsai_at_txn.get(&bitvec).unwrap().unwrap(), pair1.1);
        });

        s.spawn(|| {
            let bonsai_at_txn = bonsai_storage
                .get_transactional_state(id1, bonsai_storage.get_config())
                .unwrap()
                .unwrap();
            let bitvec = BitVec::from_vec(pair1.0.clone());
            assert_eq!(bonsai_at_txn.get(&bitvec).unwrap().unwrap(), pair1.1);
        });
    });
    bonsai_storage
        .get(&BitVec::from_vec(vec![1, 2, 2]))
        .unwrap();
    let pair2 = (
        vec![1, 2, 3],
        Felt252Wrapper::from_hex_be("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_storage
        .insert(&BitVec::from_vec(pair2.0.clone()), &pair2.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
}
```
