#![cfg(feature = "std")]
use crate::{
    databases::{create_rocks_db, HashMapDb, RocksDB, RocksDBConfig},
    id::{BasicId, BasicIdBuilder},
    BonsaiStorage, BonsaiStorageConfig, Change,
};
use bitvec::{order::Msb0, vec::BitVec, view::BitView};
use starknet_types_core::{felt::Felt, hash::Pedersen};

#[test]
fn basics() {
    let identifier = vec![];
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
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let pair2 = (
        vec![1, 2, 2],
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair2.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let pair3 = (
        vec![1, 2, 3],
        Felt::from_hex("0x66342762FD54D033c195fec3ce2568b62052e").unwrap(),
    );
    let bitvec = BitVec::from_vec(pair3.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair3.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let bitvec = BitVec::from_vec(vec![1, 2, 1]);
    bonsai_storage.remove(&identifier, &bitvec).unwrap();
    assert_eq!(
        bonsai_storage
            .get(&identifier, &BitVec::from_vec(vec![1, 2, 1]))
            .unwrap(),
        None
    );
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    assert_eq!(
        bonsai_storage
            .get(&identifier, &BitVec::from_vec(vec![1, 2, 1]))
            .unwrap(),
        None
    );
    assert_eq!(bonsai_storage.get_keys(&identifier).unwrap().len(), 2);
}

#[test]
fn root_hash_similar_rocks_db() {
    let identifier = vec![];
    let root_hash_1 = {
        let tempdir = tempfile::tempdir().unwrap();
        let db = create_rocks_db(tempdir.path()).unwrap();
        let config = BonsaiStorageConfig::default();
        let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
            BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
        let mut id_builder = BasicIdBuilder::new();
        let pair1 = (
            vec![1, 2, 1],
            Felt::from_hex("0x2acf9d2ae5a475818075672b04e317e9da3d5180fed2c5f8d6d8a5fd5a92257")
                .unwrap(),
        );
        let bitvec = BitVec::from_vec(pair1.0.clone());
        bonsai_storage
            .insert(&identifier, &bitvec, &pair1.1)
            .unwrap();
        let pair2 = (
            vec![1, 2, 2],
            Felt::from_hex("0x100bd6fbfced88ded1b34bd1a55b747ce3a9fde9a914bca75571e4496b56443")
                .unwrap(),
        );
        let bitvec = BitVec::from_vec(pair2.0.clone());
        bonsai_storage
            .insert(&identifier, &bitvec, &pair2.1)
            .unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        let pair3 = (
            vec![1, 2, 3],
            Felt::from_hex("0x00a038cda302fedbc4f6117648c6d3faca3cda924cb9c517b46232c6316b152f")
                .unwrap(),
        );
        let bitvec = BitVec::from_vec(pair3.0.clone());
        bonsai_storage
            .insert(&identifier, &bitvec, &pair3.1)
            .unwrap();
        let pair4 = (
            vec![1, 2, 4],
            Felt::from_hex("0x02808c7d8f3745e55655ad3f51f096d0c06a41f3d76caf96bad80f9be9ced171")
                .unwrap(),
        );
        let bitvec = BitVec::from_vec(pair4.0.clone());
        bonsai_storage
            .insert(&identifier, &bitvec, &pair4.1)
            .unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        bonsai_storage.root_hash(&identifier).unwrap()
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
            Felt::from_hex("0x00a038cda302fedbc4f6117648c6d3faca3cda924cb9c517b46232c6316b152f")
                .unwrap(),
        );
        let bitvec = BitVec::from_vec(pair1.0.clone());
        bonsai_storage
            .insert(&identifier, &bitvec, &pair1.1)
            .unwrap();
        let pair2 = (
            vec![1, 2, 4],
            Felt::from_hex("0x02808c7d8f3745e55655ad3f51f096d0c06a41f3d76caf96bad80f9be9ced171")
                .unwrap(),
        );
        let bitvec = BitVec::from_vec(pair2.0.clone());
        bonsai_storage
            .insert(&identifier, &bitvec, &pair2.1)
            .unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        bonsai_storage.root_hash(&identifier).unwrap()
    };
    println!("root_hash_1: {:?}", root_hash_1.to_string());
    println!("root_hash_2: {:?}", root_hash_2.to_string());
    assert_ne!(root_hash_1, root_hash_2);
}

#[test]
fn starknet_specific() {
    struct ContractState {
        address: &'static str,
        state_hash: &'static str,
    }
    let identifier = vec![];

    let tempdir1 = tempfile::tempdir().unwrap();
    let db1 = create_rocks_db(tempdir1.path()).unwrap();
    let config1 = BonsaiStorageConfig::default();
    let mut bonsai_storage1: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db1, RocksDBConfig::default()), config1).unwrap();

    let tempdir2 = tempfile::tempdir().unwrap();
    let db2 = create_rocks_db(tempdir2.path()).unwrap();
    let config2 = BonsaiStorageConfig::default();
    let mut bonsai_storage2: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db2, RocksDBConfig::default()), config2).unwrap();
    let mut id_builder = BasicIdBuilder::new();

    let contract_states = vec![
        ContractState {
            address: "0x020cfa74ee3564b4cd5435cdace0f9c4d43b939620e4a0bb5076105df0a626c6",
            state_hash: "0x3a1606fc1a168e11bc31605aa32265a1a887c185feebb255a56bcac189fd5b6",
        },
        ContractState {
            address: "0x06ee3440b08a9c805305449ec7f7003f27e9f7e287b83610952ec36bdc5a6bae",
            state_hash: "0x4fc78cbac87f833e56c91dfd6eda5be3362204d86d24f1e1e81577d509f963b",
        },
    ];

    for contract_state in contract_states {
        let key = contract_state.address;
        let value = contract_state.state_hash;
        let key = Felt::from_hex(key).unwrap().to_bytes_be().view_bits()[5..].to_bitvec();
        let value = Felt::from_hex(value).unwrap();
        bonsai_storage1
            .insert(&identifier, &key, &value)
            .expect("Failed to insert storage update into trie");
        bonsai_storage2
            .insert(&identifier, &key, &value)
            .expect("Failed to insert storage update into trie");
    }

    let id = id_builder.new_id();
    bonsai_storage1
        .commit(id)
        .expect("Failed to commit to bonsai storage");

    let contract_states = vec![ContractState {
        address: "0x06538fdd3aa353af8a87f5fe77d1f533ea82815076e30a86d65b72d3eb4f0b80",
        state_hash: "0x2acf9d2ae5a475818075672b04e317e9da3d5180fed2c5f8d6d8a5fd5a92257",
    }];

    for contract_state in contract_states {
        let key = contract_state.address;
        let value = contract_state.state_hash;
        let key = Felt::from_hex(key).unwrap().to_bytes_be().view_bits()[5..].to_bitvec();
        let value = Felt::from_hex(value).unwrap();

        bonsai_storage1
            .insert(&identifier, &key, &value)
            .expect("Failed to insert storage update into trie");
        bonsai_storage2
            .insert(&identifier, &key, &value)
            .expect("Failed to insert storage update into trie");
    }

    let id = id_builder.new_id();
    bonsai_storage1
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash1 = bonsai_storage1
        .root_hash(&identifier)
        .expect("Failed to get root hash");

    bonsai_storage2
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash2 = bonsai_storage2
        .root_hash(&identifier)
        .expect("Failed to get root hash");
    assert_eq!(root_hash1, root_hash2);
}

#[test]
fn root_hash_similar_hashmap_db() {
    let identifier = vec![];
    let root_hash_1 = {
        let db = HashMapDb::<BasicId>::default();
        let config = BonsaiStorageConfig::default();
        let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
            BonsaiStorage::new(db, config).unwrap();
        let mut id_builder = BasicIdBuilder::new();
        let pair1 = (
            vec![1, 2, 1],
            Felt::from_hex("0x2acf9d2ae5a475818075672b04e317e9da3d5180fed2c5f8d6d8a5fd5a92257")
                .unwrap(),
        );
        let bitvec = BitVec::from_vec(pair1.0.clone());
        bonsai_storage
            .insert(&identifier, &bitvec, &pair1.1)
            .unwrap();
        let pair2 = (
            vec![1, 2, 2],
            Felt::from_hex("0x100bd6fbfced88ded1b34bd1a55b747ce3a9fde9a914bca75571e4496b56443")
                .unwrap(),
        );
        let bitvec = BitVec::from_vec(pair2.0.clone());
        bonsai_storage
            .insert(&identifier, &bitvec, &pair2.1)
            .unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        let pair3 = (
            vec![1, 2, 3],
            Felt::from_hex("0x00a038cda302fedbc4f6117648c6d3faca3cda924cb9c517b46232c6316b152f")
                .unwrap(),
        );
        let bitvec = BitVec::from_vec(pair3.0.clone());
        bonsai_storage
            .insert(&identifier, &bitvec, &pair3.1)
            .unwrap();
        let pair4 = (
            vec![1, 2, 4],
            Felt::from_hex("0x02808c7d8f3745e55655ad3f51f096d0c06a41f3d76caf96bad80f9be9ced171")
                .unwrap(),
        );
        let bitvec = BitVec::from_vec(pair4.0.clone());
        bonsai_storage
            .insert(&identifier, &bitvec, &pair4.1)
            .unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        bonsai_storage.root_hash(&identifier).unwrap()
    };
    let root_hash_2 = {
        let db = HashMapDb::<BasicId>::default();
        let config = BonsaiStorageConfig::default();
        let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
            BonsaiStorage::new(db, config).unwrap();
        let mut id_builder = BasicIdBuilder::new();
        let pair1 = (
            vec![1, 2, 3],
            Felt::from_hex("0x00a038cda302fedbc4f6117648c6d3faca3cda924cb9c517b46232c6316b152f")
                .unwrap(),
        );
        let bitvec = BitVec::from_vec(pair1.0.clone());
        bonsai_storage
            .insert(&identifier, &bitvec, &pair1.1)
            .unwrap();
        let pair2 = (
            vec![1, 2, 4],
            Felt::from_hex("0x02808c7d8f3745e55655ad3f51f096d0c06a41f3d76caf96bad80f9be9ced171")
                .unwrap(),
        );
        let bitvec = BitVec::from_vec(pair2.0.clone());
        bonsai_storage
            .insert(&identifier, &bitvec, &pair2.1)
            .unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        bonsai_storage.root_hash(&identifier).unwrap()
    };
    println!("root_hash_1: {:?}", root_hash_1.to_string());
    println!("root_hash_2: {:?}", root_hash_2.to_string());
    assert_ne!(root_hash_1, root_hash_2);
}

#[test]
fn double_insert() {
    let identifier = vec![];
    struct ContractState {
        address: &'static str,
        state_hash: &'static str,
    }
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
    let mut id_builder = BasicIdBuilder::new();
    let contract_states = vec![
        ContractState {
            address: "0x0000000000000000000000000000000000000000000000000000000000000005",
            state_hash: "0x000000000000000000000000000000000000000000000000000000000000022b",
        },
        ContractState {
            address: "0x0313ad57fdf765addc71329abf8d74ac2bce6d46da8c2b9b82255a5076620300",
            state_hash: "0x04e7e989d58a17cd279eca440c5eaa829efb6f9967aaad89022acbe644c39b36",
        },
        // This seems to be what is causing the problem in case of double insertions.
        // Other value are fine
        ContractState {
            address: "0x313ad57fdf765addc71329abf8d74ac2bce6d46da8c2b9b82255a5076620301",
            state_hash: "0x453ae0c9610197b18b13645c44d3d0a407083d96562e8752aab3fab616cecb0",
        },
        ContractState {
            address: "0x05aee31408163292105d875070f98cb48275b8c87e80380b78d30647e05854d5",
            state_hash: "0x00000000000000000000000000000000000000000000000000000000000007e5",
        },
        ContractState {
            address: "0x06cf6c2f36d36b08e591e4489e92ca882bb67b9c39a3afccf011972a8de467f0",
            state_hash: "0x07ab344d88124307c07b56f6c59c12f4543e9c96398727854a322dea82c73240",
        },
    ];
    for contract_state in contract_states {
        let key = contract_state.address;
        let value = contract_state.state_hash;

        let key = Felt::from_hex(key).unwrap();
        let bitkey = key.to_bytes_be().view_bits()[5..].to_bitvec();
        let value = Felt::from_hex(value).unwrap();
        bonsai_storage
            .insert(&identifier, &bitkey, &value)
            .expect("Failed to insert storage update into trie");
        // fails here for key 0x313ad57fdf765addc71329abf8d74ac2bce6d46da8c2b9b82255a5076620301
        // and value 0x453ae0c9610197b18b13645c44d3d0a407083d96562e8752aab3fab616cecb0
        bonsai_storage
            .insert(&identifier, &bitkey, &value)
            .unwrap_or_else(|_| {
                panic!(
                    "Failed to insert storage update into trie for key {key:#x} & value {value:#x}"
                )
            });
    }
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let root_hash = bonsai_storage.root_hash(&identifier).unwrap();
    println!("root hash: {root_hash:#x}");
}

#[test]
fn double_identifier() {
    let identifier = vec![2, 3];
    let identifier2 = vec![1, 3, 1];
    struct ContractState {
        address: &'static str,
        state_hash: &'static str,
    }
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
    let mut id_builder = BasicIdBuilder::new();
    let contract_states = vec![
        ContractState {
            address: "0x0000000000000000000000000000000000000000000000000000000000000005",
            state_hash: "0x000000000000000000000000000000000000000000000000000000000000022b",
        },
        ContractState {
            address: "0x0313ad57fdf765addc71329abf8d74ac2bce6d46da8c2b9b82255a5076620300",
            state_hash: "0x04e7e989d58a17cd279eca440c5eaa829efb6f9967aaad89022acbe644c39b36",
        },
        // This seems to be what is causing the problem in case of double insertions.
        // Other value are fine
        ContractState {
            address: "0x313ad57fdf765addc71329abf8d74ac2bce6d46da8c2b9b82255a5076620301",
            state_hash: "0x453ae0c9610197b18b13645c44d3d0a407083d96562e8752aab3fab616cecb0",
        },
        ContractState {
            address: "0x05aee31408163292105d875070f98cb48275b8c87e80380b78d30647e05854d5",
            state_hash: "0x00000000000000000000000000000000000000000000000000000000000007e5",
        },
        ContractState {
            address: "0x06cf6c2f36d36b08e591e4489e92ca882bb67b9c39a3afccf011972a8de467f0",
            state_hash: "0x07ab344d88124307c07b56f6c59c12f4543e9c96398727854a322dea82c73240",
        },
    ];
    for contract_state in contract_states {
        let key = contract_state.address;
        let value = contract_state.state_hash;

        let key = Felt::from_hex(key).unwrap();
        let bitkey = key.to_bytes_be().view_bits()[5..].to_bitvec();
        let value = Felt::from_hex(value).unwrap();
        bonsai_storage
            .insert(&identifier, &bitkey, &value)
            .expect("Failed to insert storage update into trie");
        // fails here for key 0x313ad57fdf765addc71329abf8d74ac2bce6d46da8c2b9b82255a5076620301
        // and value 0x453ae0c9610197b18b13645c44d3d0a407083d96562e8752aab3fab616cecb0
        bonsai_storage
            .insert(&identifier2, &bitkey, &value)
            .unwrap_or_else(|_| {
                panic!(
                    "Failed to insert storage update into trie for key {key:#x} & value {value:#x}"
                )
            });
    }
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let root_hash = bonsai_storage.root_hash(&identifier).unwrap();
    println!("root hash: {root_hash:#x}");
    let root_hash2 = bonsai_storage.root_hash(&identifier2).unwrap();
    assert_eq!(root_hash, root_hash2);
    assert_eq!(
        bonsai_storage.get_keys(&identifier).unwrap(),
        bonsai_storage.get_keys(&identifier2).unwrap()
    );
    assert_eq!(bonsai_storage.get_keys(&identifier).unwrap().len(), 5);
}

#[test]
fn get_changes() {
    let identifier = vec![];
    let tempdir = tempfile::tempdir().unwrap();
    let db = create_rocks_db(tempdir.path()).unwrap();
    let config = BonsaiStorageConfig::default();
    let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> =
        BonsaiStorage::new(RocksDB::new(&db, RocksDBConfig::default()), config).unwrap();
    let mut id_builder = BasicIdBuilder::new();
    let pair1 = (vec![1, 2, 1], Felt::from_hex("0x01").unwrap());
    let bitvec = BitVec::from_vec(pair1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1.1)
        .unwrap();
    bonsai_storage.commit(id_builder.new_id()).unwrap();
    let pair2 = (vec![1, 2, 2], Felt::from_hex("0x01").unwrap());
    let bitvec = BitVec::from_vec(pair2.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair2.1)
        .unwrap();
    let pair1_edited_1 = (vec![1, 2, 1], Felt::from_hex("0x02").unwrap());
    let bitvec = BitVec::from_vec(pair1_edited_1.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1_edited_1.1)
        .unwrap();
    let pair1_edited_2 = (vec![1, 2, 1], Felt::from_hex("0x03").unwrap());
    let bitvec = BitVec::from_vec(pair1_edited_2.0.clone());
    bonsai_storage
        .insert(&identifier, &bitvec, &pair1_edited_2.1)
        .unwrap();
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

fn keyer(felt: Felt) -> BitVec<u8, Msb0> {
    felt.to_bytes_be().view_bits()[5..].to_bitvec()
}

#[test]
fn test_insert_zero() {
    let config = BonsaiStorageConfig::default();
    let bonsai_db = HashMapDb::<BasicId>::default();
    let mut bonsai_storage = BonsaiStorage::<_, _, Pedersen>::new(bonsai_db, config)
        .expect("Failed to create bonsai storage");
    let identifier =
        "0x056e4fed965fccd7fb01fcadd827470338f35ced62275328929d0d725b5707ba".as_bytes();

    // Insert Block 3 storage changes for contract `0x056e4fed965fccd7fb01fcadd827470338f35ced62275328929d0d725b5707ba`
    let block_3 = [
        ("0x5", "0x456"),
        (
            "0x378e096bb5e74b0f4ca78660a6b49b4a8035e571b024c018713c80b4b969735",
            "0x205d119502a165dae3830f627fa93fbdf5bfb13edd8f00e4c72621d0cda24",
        ),
        (
            "0x41139bbf557d599fe8e96983251ecbfcb5bf4c4138c85946b0c4a6a68319f24",
            "0x7eec291f712520293664c7e3a8bb39ab00babf51cb0d9c1fb543147f37b485f",
        ),
        (
            "0x77ae79c60260b3e48516a7da1aa173ac2765a5ced420f8ffd1539c394fbc03c",
            "0x6025343ab6a7ac36acde4eba3b6fc21f53d5302ee26e6f28e8de5a62bbfd847",
        ),
        (
            "0x751901aac66fdc1f455c73022d02f1c085602cd0c9acda907cfca5418769e9c",
            "0x3f23078d48a4bf1d5f8ca0348f9efe9300834603625a379cae5d6d81100adef",
        ),
        (
            "0x751901aac66fdc1f455c73022d02f1c085602cd0c9acda907cfca5418769e9d",
            "0xbd858a06904cadc3787ecbad97409606dcee50ea6fc30b94930bcf3d8843d5",
        ),
    ];

    for (key_hex, value_hex) in block_3.iter() {
        let key: Felt = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let mut id_builder = BasicIdBuilder::new();
    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!(
        "Expected: 0x069064A05C14A9A2B4ED81C479C14D30872A9AE9CE2DEA8E4B4509542C2DCC1F\nFound: {root_hash:#x}",
    );
    assert_eq!(
        root_hash,
        Felt::from_hex("0x069064A05C14A9A2B4ED81C479C14D30872A9AE9CE2DEA8E4B4509542C2DCC1F")
            .unwrap()
    );

    // Insert Block 4 storage changes for contract `0x056e4fed965fccd7fb01fcadd827470338f35ced62275328929d0d725b5707ba`
    let block_4 = [
        ("0x5", "0x0"), // Inserting key = 0x0
        (
            "0x4b81c1bca2d1b7e08535a5abe231b2e94399674db5e8f1d851fd8f4af4abd34",
            "0x7c7",
        ),
        (
            "0x6f8cf54aaec1f42d5f3868d597fcd7393da888264dc5a6e93c7bd528b6d6fee",
            "0x7e5",
        ),
        (
            "0x2a315469199dfde4b05906db8c33f6962916d462d8f1cf5252b748dfa174a20",
            "0xdae79d0308bb710af439eb36e82b405dc2bca23b351d08b4867d9525226e9d",
        ),
        (
            "0x2d1ed96c7561dd8e5919657790ffba8473b80872fea3f7ef8279a7253dc3b33",
            "0x750387f4d66b0e9be1f2f330e8ad309733c46bb74e0be4df0a8c58fb4e89a25",
        ),
        (
            "0x6a93bcb89fc1f31fa544377c7de6de1dd3e726e1951abc95c4984995e84ad0d",
            "0x7e5",
        ),
        (
            "0x6b3b4780013c33cdca6799e8aa3ef922b64f5a2d356573b33693d81504deccf",
            "0x7c7",
        ),
    ];

    for (key_hex, value_hex) in block_4.iter() {
        let key: Felt = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!(
        "Expected: 0x0112998A41A3A2C720E758F82D184E4C39E9382620F12076B52C516D14622E57\nFound: {root_hash:#x}",
    );
    assert_eq!(
        root_hash,
        Felt::from_hex("0x0112998A41A3A2C720E758F82D184E4C39E9382620F12076B52C516D14622E57")
            .unwrap()
    );

    // Insert Block 5 storage changes for contract `0x056e4fed965fccd7fb01fcadd827470338f35ced62275328929d0d725b5707ba`
    let block_5 = [("0x5", "0x456")];

    for (key_hex, value_hex) in block_5.iter() {
        let key: Felt = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!(
        "Expected: 0x072E79A6F71E3E63D7DE40EDF4322A22E64388D4D5BFE817C1271C78028B73BF\nFound: {root_hash:#x}"
    );
    assert_eq!(
        root_hash,
        Felt::from_hex("0x072E79A6F71E3E63D7DE40EDF4322A22E64388D4D5BFE817C1271C78028B73BF")
            .unwrap()
    );
}

#[test]
fn test_block_7_starknet() {
    let config = BonsaiStorageConfig::default();
    let bonsai_db = HashMapDb::<BasicId>::default();
    let mut bonsai_storage = BonsaiStorage::<_, _, Pedersen>::new(bonsai_db, config)
        .expect("Failed to create bonsai storage");
    let identifier =
        "0x056e4fed965fccd7fb01fcadd827470338f35ced62275328929d0d725b5707ba".as_bytes();

    // Insert Block 3 storage changes for contract `0x4d56b8ac0ed905936da10323328cba5def12957a2936920f043d8bf6a1e902d`
    let block_3 = [
        (
            "0x67c2665fbdd32ded72c0665f9658c05a5f9233c8de2002b3eba8ae046174efd",
            "0x2221def5413ed3e128051d5dff3ec816dbfb9db4454b98f4aa47804cb7a13d2",
        ),
        ("0x5", "0x66"),
        (
            "0x101c2b102c8eb6bf091f5debcf97d8edde85983e23f9778e9cabbe0b5a4f997",
            "0x99a58a9612fe930f39c4c399b6be14e8bb7c8229d06eab8d0a3a97877a6667",
        ),
        (
            "0x1aabd3b2e12959bab2c4ab530c1d8f0e675e0dc5ab29d1f10b7f1a154cabef9",
            "0x41d4ae0ba9013f2f6e1551b62a9c9187053727e0e65217be97eae8922d5b2df",
        ),
        (
            "0x1aabd3b2e12959bab2c4ab530c1d8f0e675e0dc5ab29d1f10b7f1a154cabefa",
            "0x6eda96627bd3de7af5b4f932ff1e858bd396c897229d64b6dd3f0f936f0ea17",
        ),
    ];

    for (key_hex, value_hex) in block_3.iter() {
        let key = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let mut id_builder = BasicIdBuilder::new();
    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!("Expected: 0x0297DE74ABD178CAF7EA2F1AE1B4588CA7433B1B11A98172B6F56E3E02739FD0\nFound: {root_hash:#x}");
    assert_eq!(
        root_hash,
        Felt::from_hex("0x0297DE74ABD178CAF7EA2F1AE1B4588CA7433B1B11A98172B6F56E3E02739FD0")
            .unwrap()
    );

    // Insert Block 4 storage changes for contract `0x4d56b8ac0ed905936da10323328cba5def12957a2936920f043d8bf6a1e902d`
    let block_4 = [
        (
            "0x3c14ddc99b06b00340bffd81ef1c4e10f74b800a911ee22c22bb28e4b516da5",
            "0x7e5",
        ),
        ("0x5", "0x64"),
        (
            "0x5201dd2a5f567a653e9a2b7a62816919d0d695d1e2f39d516f9befda30da720",
            "0x29ed6ea046ebe50aaacb9cd6477ac368644c8f4242ee0687d31f6c2ac20c146",
        ),
        (
            "0x5b3856459ac954d3fd24d85924d978263709880e3ee4cafdfe0b7c95ee6b26a",
            "0x4c90411b3376d5230a88496e58acf58c19431d52b89f1ab91924075f4b35ac1",
        ),
        (
            "0x5b3856459ac954d3fd24d85924d978263709880e3ee4cafdfe0b7c95ee6b26b",
            "0x72a56d83fab34872a880dd35d936117a084b928fb9d47306abb2558472633c",
        ),
        (
            "0x6a93bcb89fc1f31fa544377c7de6de1dd3e726e1951abc95c4984995e84ad0d",
            "0x7c7",
        ),
        (
            "0x6f8cf54aaec1f42d5f3868d597fcd7393da888264dc5a6e93c7bd528b6d6fee",
            "0x7c7",
        ),
        (
            "0x6b30a5f1341c0c949f847afe7f761a6ea8cdc3337baa20e68a2891f62389052",
            "0x7e5",
        ),
        (
            "0x6b3b4780013c33cdca6799e8aa3ef922b64f5a2d356573b33693d81504deccf",
            "0x7e5",
        ),
        (
            "0x6f649e057570e0f3cc710d260c2067297542f8e18407a7e75008808e12e6099",
            "0x61395ebfa1746f9449711a7e361254ddb90f642861807b7e5e05276c11033ec",
        ),
        (
            "0x6f649e057570e0f3cc710d260c2067297542f8e18407a7e75008808e12e609a",
            "0x304d0ec8cc0ea6faf0f7ad67903bcffc6bc4474d25f93e1c961b239370b8c07",
        ),
    ];

    for (key_hex, value_hex) in block_4.iter() {
        let key = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!("Expected: 0x07A4CA1440AF3858CEB11386BA7E2A0FC553BB73E741043218845D820009BCCB\nFound: {root_hash:#x}");
    assert_eq!(
        root_hash,
        Felt::from_hex("0x07A4CA1440AF3858CEB11386BA7E2A0FC553BB73E741043218845D820009BCCB")
            .unwrap()
    );

    // Insert Block 5 storage changes for contract `0x4d56b8ac0ed905936da10323328cba5def12957a2936920f043d8bf6a1e902d`
    let block_5 = [
        (
            "0x272cd29c23c7fd72ef13352ac037c6fabfee4c03056ea413c326be6501b4f31",
            "0x7c7",
        ),
        (
            "0x2bb6a7dd9cbb9cec8fdad9c0557bd539683f7ea65d4f14d41fe4d72311775e3",
            "0x7e5",
        ),
    ];

    for (key_hex, value_hex) in block_5.iter() {
        let key = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!("Expected: 0x002363DCD04D065C6B50A4D46F930EBC91AC7F4B15DCF1B0A8D0165B0BA0F143\nFound: {root_hash:#x}");
    assert_eq!(
        root_hash,
        Felt::from_hex("0x002363DCD04D065C6B50A4D46F930EBC91AC7F4B15DCF1B0A8D0165B0BA0F143")
            .unwrap()
    );

    // Insert Block 6 storage changes for contract `0x4d56b8ac0ed905936da10323328cba5def12957a2936920f043d8bf6a1e902d`
    let block_6 = [("0x5", "0x22b")];

    for (key_hex, value_hex) in block_6.iter() {
        let key = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!("Expected: 0x00C656C01BB43291BEA976CEACE3AFE89A5621045E3B6F23E4BCFFFBB4B66832\nFound: {root_hash:#x}");
    assert_eq!(
        root_hash,
        Felt::from_hex("0x00C656C01BB43291BEA976CEACE3AFE89A5621045E3B6F23E4BCFFFBB4B66832")
            .unwrap()
    );

    // Insert Block 6 storage changes for contract `0x4d56b8ac0ed905936da10323328cba5def12957a2936920f043d8bf6a1e902d`
    let block_7 = [("0x5", "0x0")];

    for (key_hex, value_hex) in block_7.iter() {
        let key = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!("Expected: 0x032C61E78534A30DD005DB4B9136AA64893CC2F6E10C4535DD6F29BFB2ADC726\nFound: {root_hash:#x}");
    assert_eq!(
        root_hash,
        Felt::from_hex("0x032C61E78534A30DD005DB4B9136AA64893CC2F6E10C4535DD6F29BFB2ADC726")
            .unwrap()
    );
}

#[test]
fn test_block_7_starknet_2() {
    let config = BonsaiStorageConfig::default();
    let bonsai_db = HashMapDb::<BasicId>::default();
    let mut bonsai_storage = BonsaiStorage::<_, _, Pedersen>::new(bonsai_db, config)
        .expect("Failed to create bonsai storage");
    let identifier = "0x421203c58e1b4a6c3675be26cfaa18d2b6b42695ca206be1f08ce29f7f1bc7c".as_bytes();

    // Insert Block 5 storage changes for contract `0x421203c58e1b4a6c3675be26cfaa18d2b6b42695ca206be1f08ce29f7f1bc7c`
    let block_5 = [
        (
            "0x2bb6a7dd9cbb9cec8fdad9c0557bd539683f7ea65d4f14d41fe4d72311775e3",
            "0x7c7",
        ),
        (
            "0x584d53558c6731da8923f60f2d182027312ffa4e811e7eddc6401232d33400e",
            "0x29bc2bad472c81f00b7873d7d27a68d63dc9ebd3a3661e2b4c3d6c90d732454",
        ),
        (
            "0x6c27ff92eab8802ca5141a60a5699e5075725d5526752c5fb368c12582af00c",
            "0x645a108cc9b963369b91cad8a8b5c2ce774b79e871368d301d518012925abc6",
        ),
        ("0x5", "0x66"),
        (
            "0x744f7d93c67c2ac6fbcdf632d530cebdbffa112d0cfacce28ed5773babfba60",
            "0x2a49283d206395239d0c1d505a8ba2f446419e58a1fd40caccf796e810759d5",
        ),
        (
            "0x11f391c712bb4996774106b93766bc49f8bdb29b416cae0da0d981752c1a28b",
            "0x43f3925b460d387343381e31e2f9299100609bc833f289bfd67316a0a06ce40",
        ),
        (
            "0x11f391c712bb4996774106b93766bc49f8bdb29b416cae0da0d981752c1a28c",
            "0x2b72713e2fc2dec7cfe8e7c428f02728a031f17f876bb50841d4ee3eb12834",
        ),
        (
            "0x66631ce6af4e11972e05bed46e9b20a5480ffea4ae2a4d95e1d71fb37f25c0",
            "0x1329ffd6765c348b5e7195b777241cf5eb84e438c0f5fa3acb5800ada846332",
        ),
    ];

    for (key_hex, value_hex) in block_5.iter() {
        let key = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let mut id_builder = BasicIdBuilder::new();
    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!("Expected: 0x03846F4AE281ADBCC68518766579DB77C27EF31955E9FC3183C397C2731A7627\nFound: {root_hash:#x}");
    assert_eq!(
        root_hash,
        Felt::from_hex("0x03846F4AE281ADBCC68518766579DB77C27EF31955E9FC3183C397C2731A7627")
            .unwrap()
    );

    // Insert Block 6 storage changes for contract `0x421203c58e1b4a6c3675be26cfaa18d2b6b42695ca206be1f08ce29f7f1bc7c`
    let block_6 = [
        (
            "0x591192c633e49a7e6ca0aae77da4e9a1df2c6db51cabb3cc929280a44745635",
            "0x1b3479bec749469312a35a2001dc8cfaf38723c0a8763e01ad2abaefb2214e5",
        ),
        (
            "0x58bfc110ce09fc2bcff40dbb4887bfb32f5156f2195e8f6ea22e15784c01768",
            "0x71cc8515287a6f5d8b81675bc7e41ca1fcd75afcc60984701033f0cdd05acd",
        ),
        (
            "0x58bfc110ce09fc2bcff40dbb4887bfb32f5156f2195e8f6ea22e15784c01769",
            "0x6a8a49d797b80ef2be0ec8a72f71dccb655c07297f95e022a26a65787c3199c",
        ),
    ];

    for (key_hex, value_hex) in block_6.iter() {
        let key = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!("Expected: 0x06E02FE529D3CBDCC5324D0981F991E777DAFC3F0C24E7CB56CE3D379BE9B510\nFound: {root_hash:#x}");
    assert_eq!(
        root_hash,
        Felt::from_hex("0x06E02FE529D3CBDCC5324D0981F991E777DAFC3F0C24E7CB56CE3D379BE9B510")
            .unwrap()
    );

    // Insert Block 6 storage changes for contract `0x421203c58e1b4a6c3675be26cfaa18d2b6b42695ca206be1f08ce29f7f1bc7c`
    let block_7 = [("0x5", "0x0")];

    for (key_hex, value_hex) in block_7.iter() {
        let key = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!("Expected: 0x0528E360EA90E94F670451A76A7698900F0F7C1F2E88583F8B0162D486BF7947\nFound: {root_hash:#x}");
    assert_eq!(
        root_hash,
        Felt::from_hex("0x0528E360EA90E94F670451A76A7698900F0F7C1F2E88583F8B0162D486BF7947")
            .unwrap()
    )
}

#[test]
fn test_block_9() {
    let config = BonsaiStorageConfig::default();
    let bonsai_db = HashMapDb::<BasicId>::default();
    let mut bonsai_storage = BonsaiStorage::<_, _, Pedersen>::new(bonsai_db, config)
        .expect("Failed to create bonsai storage");
    let identifier =
        "0x06F3C934BA4EC49245CB9A42FC715E4D589AA502AF69BE13916127A538D525CE".as_bytes();

    // Insert Block 8 storage changes for contract `0x06F3C934BA4EC49245CB9A42FC715E4D589AA502AF69BE13916127A538D525CE`
    let block_8 = [
        ("0x5", "0x456"),
        (
            "0x4b788ad12d2e47b2be358d61cc38d813aa79165ddbc0b29d4878ef0fbc18c15",
            "0x612af3160e28962cb3dd6146a9c2f7bd7adeea1fddd39f767d936c7b5bcca97",
        ),
    ];

    for (key_hex, value_hex) in block_8.iter() {
        let key = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let mut id_builder = BasicIdBuilder::new();
    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!("Expected: 0x010AA5D1D36847AE64BA074B3A878BFD1A9AEAA952F6777C727EEA6AE6B2C99F\nFound: {root_hash:#x}");
    assert_eq!(
        root_hash,
        Felt::from_hex("0x010AA5D1D36847AE64BA074B3A878BFD1A9AEAA952F6777C727EEA6AE6B2C99F")
            .unwrap()
    );

    // Insert Block 9 storage changes for contract `0x06F3C934BA4EC49245CB9A42FC715E4D589AA502AF69BE13916127A538D525CE`
    let block_9 = [("0x5", "0x0")];

    for (key_hex, value_hex) in block_9.iter() {
        let key = Felt::from_hex(key_hex).unwrap();
        let value = Felt::from_hex(value_hex).unwrap();
        bonsai_storage
            .insert(identifier, keyer(key).as_bitslice(), &value)
            .expect("Failed to insert storage update into trie");
    }

    let id = id_builder.new_id();
    bonsai_storage
        .commit(id)
        .expect("Failed to commit to bonsai storage");
    let root_hash = bonsai_storage
        .root_hash(identifier)
        .expect("Failed to get root hash");

    println!("Expected: 0x00072F7E2EC1A2F05342503B49AECD83E14884AE374A8570F2F6F7B868CF94AE\nFound: {root_hash:#x}");
    assert_eq!(
        root_hash,
        Felt::from_hex("0x00072F7E2EC1A2F05342503B49AECD83E14884AE374A8570F2F6F7B868CF94AE")
            .unwrap()
    );
}
