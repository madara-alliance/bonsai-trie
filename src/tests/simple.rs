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
    let identifier = vec![];
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
