# bonsai-trie

![example workflow](https://github.com/keep-starknet-strange/bonsai-trie/actions/workflows/check_lint.yml/badge.svg) ![example workflow](https://github.com/keep-starknet-strange/bonsai-trie/actions/workflows/test.yml/badge.svg)

[![Exploration_Team](https://img.shields.io/badge/Exploration_Team-29296E.svg?&style=for-the-badge&logo=data:image/svg%2bxml;base64,PD94bWwgdmVyc2lvbj0iMS4wIiBlbmNvZGluZz0iVVRGLTgiPz48c3ZnIGlkPSJhIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAxODEgMTgxIj48ZGVmcz48c3R5bGU+LmJ7ZmlsbDojZmZmO308L3N0eWxlPjwvZGVmcz48cGF0aCBjbGFzcz0iYiIgZD0iTTE3Ni43Niw4OC4xOGwtMzYtMzcuNDNjLTEuMzMtMS40OC0zLjQxLTIuMDQtNS4zMS0xLjQybC0xMC42MiwyLjk4LTEyLjk1LDMuNjNoLjc4YzUuMTQtNC41Nyw5LjktOS41NSwxNC4yNS0xNC44OSwxLjY4LTEuNjgsMS44MS0yLjcyLDAtNC4yN0w5Mi40NSwuNzZxLTEuOTQtMS4wNC00LjAxLC4xM2MtMTIuMDQsMTIuNDMtMjMuODMsMjQuNzQtMzYsMzcuNjktMS4yLDEuNDUtMS41LDMuNDQtLjc4LDUuMThsNC4yNywxNi41OGMwLDIuNzIsMS40Miw1LjU3LDIuMDcsOC4yOS00LjczLTUuNjEtOS43NC0xMC45Ny0xNS4wMi0xNi4wNi0xLjY4LTEuODEtMi41OS0xLjgxLTQuNCwwTDQuMzksODguMDVjLTEuNjgsMi4zMy0xLjgxLDIuMzMsMCw0LjUzbDM1Ljg3LDM3LjNjMS4zNiwxLjUzLDMuNSwyLjEsNS40NCwxLjQybDExLjQtMy4xMSwxMi45NS0zLjYzdi45MWMtNS4yOSw0LjE3LTEwLjIyLDguNzYtMTQuNzYsMTMuNzNxLTMuNjMsMi45OC0uNzgsNS4zMWwzMy40MSwzNC44NGMyLjIsMi4yLDIuOTgsMi4yLDUuMTgsMGwzNS40OC0zNy4xN2MxLjU5LTEuMzgsMi4xNi0zLjYsMS40Mi01LjU3LTEuNjgtNi4wOS0zLjI0LTEyLjMtNC43OS0xOC4zOS0uNzQtMi4yNy0xLjIyLTQuNjItMS40Mi02Ljk5LDQuMyw1LjkzLDkuMDcsMTEuNTIsMTQuMjUsMTYuNzEsMS42OCwxLjY4LDIuNzIsMS42OCw0LjQsMGwzNC4zMi0zNS43NHExLjU1LTEuODEsMC00LjAxWm0tNzIuMjYsMTUuMTVjLTMuMTEtLjc4LTYuMDktMS41NS05LjE5LTIuNTktMS43OC0uMzQtMy42MSwuMy00Ljc5LDEuNjhsLTEyLjk1LDEzLjg2Yy0uNzYsLjg1LTEuNDUsMS43Ni0yLjA3LDIuNzJoLS42NWMxLjMtNS4zMSwyLjcyLTEwLjYyLDQuMDEtMTUuOGwxLjY4LTYuNzNjLjg0LTIuMTgsLjE1LTQuNjUtMS42OC02LjA5bC0xMi45NS0xNC4xMmMtLjY0LS40NS0xLjE0LTEuMDgtMS40Mi0xLjgxbDE5LjA0LDUuMTgsMi41OSwuNzhjMi4wNCwuNzYsNC4zMywuMTQsNS43LTEuNTVsMTIuOTUtMTQuMzhzLjc4LTEuMDQsMS42OC0xLjE3Yy0xLjgxLDYuNi0yLjk4LDE0LjEyLTUuNDQsMjAuNDYtMS4wOCwyLjk2LS4wOCw2LjI4LDIuNDYsOC4xNiw0LjI3LDQuMTQsOC4yOSw4LjU1LDEyLjk1LDEyLjk1LDAsMCwxLjMsLjkxLDEuNDIsMi4wN2wtMTMuMzQtMy42M1oiLz48L3N2Zz4=)](https://github.com/keep-starknet-strange)

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
    ProofNode, Membership
};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Pedersen;
use bitvec::prelude::*;

fn main() {
    // Get the underlying key-value store.
    let db = create_rocks_db("./rocksdb").unwrap();
    
    // Create a BonsaiStorage with default parameters.
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> = BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
    
    // Create a simple incremental ID builder for commit IDs.
    // This is not necessary, you can use any kind of strictly monotonically increasing value to tag your commits. 
    let mut id_builder = BasicIdBuilder::new();

    // Define an idenfitier for a trie. All insert, get, remove and root hash will use this identifier. Define multiple identifier to use multiple tries that have separate root hash.
    let identifier = vec![];
    
    // Insert an item `pair1`.
    let pair1 = (vec![1, 2, 1], Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap());
    let bitvec_1 = BitVec::from_vec(pair1.0.clone());
    bonsai_storage.insert(&identifier, &bitvec_1, &pair1.1).unwrap();

    // Insert a second item `pair2`.
    let pair2 = (vec![1, 2, 2], Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap());
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage.insert(&identifier, &bitvec, &pair2.1).unwrap();

    // Commit the insertion of `pair1` and `pair2`.
    let id1 = id_builder.new_id()
    bonsai_storage.commit(id1);

    // Insert a new item `pair3`.
    let pair3 = (vec![1, 2, 2], Felt::from_hex("0x664D033c195fec3ce2568b62052e").unwrap());
    let bitvec = BitVec::from_vec(pair3.0.clone());
    bonsai_storage.insert(&identifier, &bitvec, &pair3.1).unwrap();

    // Commit the insertion of `pair3`. Save the commit ID to the `revert_to_id` variable.
    let revert_to_id = id_builder.new_id();
    bonsai_storage.commit(revert_to_id);

    // Remove `pair3`.
    bonsai_storage.remove(&identifier, &bitvec).unwrap();

    // Commit the removal of `pair3`.
    bonsai_storage.commit(id_builder.new_id());

    // Print the root hash and item `pair1`.
    println!("root: {:#?}", bonsai_storage.root_hash(&identifier));
    println!(
        "value: {:#?}",
        bonsai_storage.get(&identifier, &bitvec_1).unwrap()
    );

    // Revert the collection state back to the commit tagged by the `revert_to_id` variable.
    bonsai_storage.revert_to(revert_to_id).unwrap();

    // Print the root hash and item `pair3`.
    println!("root: {:#?}", bonsai_storage.root_hash(&identifier));
    println!("value: {:#?}", bonsai_storage.get(&identifier, &bitvec).unwrap());

    // Launch two threads that will simultaneously take transactional states to the commit identified by `id1`,
    // asserting in both of them that the item `pair1` is present and has the right value.
    std::thread::scope(|s| {
        s.spawn(|| {
            let bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
                .get_transactional_state(id1, bonsai_storage.get_config())
                .unwrap()
                .unwrap();
            let bitvec = BitVec::from_vec(pair1.0.clone());
            assert_eq!(bonsai_at_txn.get(&identifier, &bitvec).unwrap().unwrap(), pair1.1);
        });

        s.spawn(|| {
            let bonsai_at_txn: BonsaiStorage<_, _, Pedersen> = bonsai_storage
                .get_transactional_state(id1, bonsai_storage.get_config())
                .unwrap()
                .unwrap();
            let bitvec = BitVec::from_vec(pair1.0.clone());
            assert_eq!(bonsai_at_txn.get(&identifier, &bitvec).unwrap().unwrap(), pair1.1);
        });
    });

    // Read item `pair1`.
    let pair1_val = bonsai_storage
        .get(&identifier, &BitVec::from_vec(vec![1, 2, 2]))
        .unwrap();

    // Insert a new item and commit.
    let pair4 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap(),
    );
    bonsai_storage
        .insert(&identifier, &BitVec::from_vec(pair4.0.clone()), &pair4.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let proof = bonsai_storage
        .get_proof(&identifier, &BitVec::from_vec(pair3.0.clone()))
        .unwrap();
    assert_eq!(
        BonsaiStorage::<BasicId, RocksDB<BasicId>>::verify_proof(
            bonsai_storage.root_hash(&identifier).unwrap(),
            &BitVec::from_vec(pair3.0.clone()),
            pair3.1,
            &proof
        ),
        Some(Membership::Member)
    );
}
```

## Acknowledgements

- Shout out to [Danno Ferrin](https://github.com/shemnon) and [Karim Taam](https://github.com/matkt) for their work on Bonsai. This project is heavily inspired by their work.
- Props to [MassaLabs](https://massa.net/) for the original implementation of this project.

## Resources

- [Bonsai explainer article by Karim Taam](https://hackmd.io/@kt2am/BktBblIL3)
- [Ethereum World State structure diagram](https://ethereum.stackexchange.com/questions/268/ethereum-block-architecture/6413#6413)
- [Besu Bonsai implementation in Java](https://github.com/hyperledger/besu/tree/1a7635bc3ef75c31e5c5ac050b2cd3a22d833ada/ethereum/core/src/main/java/org/hyperledger/besu/ethereum/bonsai)
- [Madara Starknet Sequencer using Substrate](https://github.com/keep-starknet-strange/madara)
- [LambdaClass Patricia Merkle Tree implementation in Rust](https://github.com/lambdaclass/merkle_patricia_tree)

