# bonsai-trie

![example workflow](https://github.com/massalabs/bonsai-trie/actions/workflows/check_lint.yml/badge.svg) ![example workflow](https://github.com/massalabs/bonsai-trie/actions/workflows/test.yml/badge.svg) [![codecov](https://codecov.io/gh/massalabs/bonsai-trie/graph/badge.svg?token=598URC32TV)](https://codecov.io/gh/massalabs/bonsai-trie)


This crate provides a storage implementation based on the Bonsai Storage implemented by [Besu](https://hackmd.io/@kt2am/BktBblIL3).
It is a key/value storage that uses a Madara Merkle Trie to store the data.

## Features

This library implements a trie-based key-value collection with the following properties:
* Optimized for holding Starknet Felt items.
* Persistance in an underlying key-value store. Defaults to RocksDB but the trie is generic on the underlying kv store.
* A Madara-compatible root hash of the collection state is maintained efficiently on insertions/deletions thanks to persistence and greedy trie updates.
* A Flat DB allowing direct access to items without requiring trie traversal. Item access complexity is inherited from the underlying key-value store without overhead.
* Commit-based system allowing tagged atomic batches of updates on the collection items.
* Trie Logs that allow efficiently reverting the collection state back to a given commit.
* Thread-safe transactional states allowing to grab and manipulate a consistent view of the collection at a given commit height. This is especially useful for processing data at a given commit height while the collection is still being written to. 
* Transactional states can be merged back into the trunk state if no collisions happpened in the meantime.

## Build:

```
cargo build
```

## Docs and examples:
```
cargo doc --open
```

## Usage example:

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
    // Get the underlying key-value store.
    let db = create_rocks_db("./rocksdb").unwrap();
    
    // Create a BonsaiStorage with default parameters.
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage = BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
    
    // Create a simple incremental ID builder for commit IDs.
    // This is not necessary, you can use any kind of strictly monotonically increasing value to tag your commits. 
    let mut id_builder = BasicIdBuilder::new();
    
    // Insert an item `pair1`.
    let pair1 = (vec![1, 2, 1], Felt252Wrapper::from_hex_be("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap());
    let bitvec_1 = BitVec::from_vec(pair1.0.clone());
    bonsai_storage.insert(&bitvec_1, &pair1.1).unwrap();

    // Insert a second item `pair2`.
    let pair2 = (vec![1, 2, 2], Felt252Wrapper::from_hex_be("0x66342762FD54D033c195fec3ce2568b62052e").unwrap());
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage.insert(&bitvec, &pair2.1).unwrap();

    // Commit the insertion of `pair1` and `pair2`.
    bonsai_storage.commit(id_builder.new_id());

    // Insert a new item `pair3`.
    let pair3 = (vec![1, 2, 2], Felt252Wrapper::from_hex_be("0x664D033c195fec3ce2568b62052e").unwrap());
    let bitvec = BitVec::from_vec(pair3.0.clone());
    bonsai_storage.insert(&bitvec, &pair3.1).unwrap();

    // Commit the insertion of `pair3`. Save the commit ID to the `revert_to_id` variable.
    let revert_to_id = id_builder.new_id();
    bonsai_storage.commit(revert_to_id);

    // Remove `pair3`.
    bonsai_storage.remove(&bitvec).unwrap();

    // Commit the removal of `pair3`.
    bonsai_storage.commit(id_builder.new_id());

    // Print the root hash and item `pair1`.
    println!("root: {:#?}", bonsai_storage.root_hash());
    println!(
        "value: {:#?}",
        bonsai_storage.get(&bitvec_1).unwrap()
    );

    // Revert the collection state back to the commit tagged by the `revert_to_id` variable.
    bonsai_storage.revert_to(revert_to_id).unwrap();

    // Print the root hash and item `pair3`.
    println!("root: {:#?}", bonsai_storage.root_hash());
    println!("value: {:#?}", bonsai_storage.get(&bitvec).unwrap());

    // Launch two threads that will simultaneously take transactional states to the commit identified by `id1`,
    // asserting in both of them that the item `pair1` is present and has the right value.
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

    // Read item `pair1`.
    let pair1_val = bonsai_storage
        .get(&BitVec::from_vec(vec![1, 2, 2]))
        .unwrap();

    // Insert a new item and commit.
    let pair4 = (
        vec![1, 2, 3],
        Felt252Wrapper::from_hex_be("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_storage
        .insert(&BitVec::from_vec(pair4.0.clone()), &pair4.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
}
```
