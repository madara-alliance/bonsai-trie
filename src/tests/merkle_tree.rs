#![cfg(all(test, feature = "std", feature = "rocksdb"))]
use bitvec::view::BitView;
use indexmap::IndexMap;
use starknet_types_core::{felt::Felt, hash::Pedersen};

use crate::{
    databases::{create_rocks_db, RocksDB, RocksDBConfig},
    id::BasicId,
    BitVec, BonsaiStorage, BonsaiStorageConfig, ByteVec,
};

#[test_log::test]
// The whole point of this test is to make sure it is possible to reconstruct the original
// keys from the data present in the db.
fn test_key_retrieval() {
    let tempdir = tempfile::tempdir().unwrap();
    let rocksdb = create_rocks_db(tempdir.path()).unwrap();
    let db = RocksDB::new(&rocksdb, RocksDBConfig::default());
    let mut bonsai =
        BonsaiStorage::<BasicId, _, Pedersen>::new(db, BonsaiStorageConfig::default(), 251);

    let block_0 = vec![
        (
            str_to_felt_bytes("0x031c887d82502ceb218c06ebb46198da3f7b92864a8223746bc836dda3e34b52"),
            vec![
                (
                    str_to_felt_bytes(
                        "0x0000000000000000000000000000000000000000000000000000000000000005",
                    ),
                    str_to_felt_bytes(
                        "0x0000000000000000000000000000000000000000000000000000000000000065",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x00cfc2e2866fd08bfb4ac73b70e0c136e326ae18fc797a2c090c8811c695577e",
                    ),
                    str_to_felt_bytes(
                        "0x05f1dd5a5aef88e0498eeca4e7b2ea0fa7110608c11531278742f0b5499af4b3",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x05aee31408163292105d875070f98cb48275b8c87e80380b78d30647e05854d5",
                    ),
                    str_to_felt_bytes(
                        "0x00000000000000000000000000000000000000000000000000000000000007c7",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x05fac6815fddf6af1ca5e592359862ede14f171e1544fd9e792288164097c35d",
                    ),
                    str_to_felt_bytes(
                        "0x00299e2f4b5a873e95e65eb03d31e532ea2cde43b498b50cd3161145db5542a5",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x05fac6815fddf6af1ca5e592359862ede14f171e1544fd9e792288164097c35e",
                    ),
                    str_to_felt_bytes(
                        "0x03d6897cf23da3bf4fd35cc7a43ccaf7c5eaf8f7c5b9031ac9b09a929204175f",
                    ),
                ),
            ],
        ),
        (
            str_to_felt_bytes("0x06ee3440b08a9c805305449ec7f7003f27e9f7e287b83610952ec36bdc5a6bae"),
            vec![
                (
                    str_to_felt_bytes(
                        "0x01e2cd4b3588e8f6f9c4e89fb0e293bf92018c96d7a93ee367d29a284223b6ff",
                    ),
                    str_to_felt_bytes(
                        "0x071d1e9d188c784a0bde95c1d508877a0d93e9102b37213d1e13f3ebc54a7751",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x0449908c349e90f81ab13042b1e49dc251eb6e3e51092d9a40f86859f7f415b0",
                    ),
                    str_to_felt_bytes(
                        "0x06cb6104279e754967a721b52bcf5be525fdc11fa6db6ef5c3a4db832acf7804",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x048cba68d4e86764105adcdcf641ab67b581a55a4f367203647549c8bf1feea2",
                    ),
                    str_to_felt_bytes(
                        "0x0362d24a3b030998ac75e838955dfee19ec5b6eceb235b9bfbeccf51b6304d0b",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x05bdaf1d47b176bfcd1114809af85a46b9c4376e87e361d86536f0288a284b65",
                    ),
                    str_to_felt_bytes(
                        "0x028dff6722aa73281b2cf84cac09950b71fa90512db294d2042119abdd9f4b87",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x05bdaf1d47b176bfcd1114809af85a46b9c4376e87e361d86536f0288a284b66",
                    ),
                    str_to_felt_bytes(
                        "0x057a8f8a019ccab5bfc6ff86c96b1392257abb8d5d110c01d326b94247af161c",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x05f750dc13ed239fa6fc43ff6e10ae9125a33bd05ec034fc3bb4dd168df3505f",
                    ),
                    str_to_felt_bytes(
                        "0x00000000000000000000000000000000000000000000000000000000000007e5",
                    ),
                ),
            ],
        ),
        (
            str_to_felt_bytes("0x0735596016a37ee972c42adef6a3cf628c19bb3794369c65d2c82ba034aecf2c"),
            vec![
                (
                    str_to_felt_bytes(
                        "0x0000000000000000000000000000000000000000000000000000000000000005",
                    ),
                    str_to_felt_bytes(
                        "0x0000000000000000000000000000000000000000000000000000000000000064",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x002f50710449a06a9fa789b3c029a63bd0b1f722f46505828a9f815cf91b31d8",
                    ),
                    str_to_felt_bytes(
                        "0x02a222e62eabe91abdb6838fa8b267ffe81a6eb575f61e96ec9aa4460c0925a2",
                    ),
                ),
            ],
        ),
        (
            str_to_felt_bytes("0x020cfa74ee3564b4cd5435cdace0f9c4d43b939620e4a0bb5076105df0a626c6"),
            vec![
                (
                    str_to_felt_bytes(
                        "0x0000000000000000000000000000000000000000000000000000000000000005",
                    ),
                    str_to_felt_bytes(
                        "0x000000000000000000000000000000000000000000000000000000000000022b",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x0313ad57fdf765addc71329abf8d74ac2bce6d46da8c2b9b82255a5076620300",
                    ),
                    str_to_felt_bytes(
                        "0x04e7e989d58a17cd279eca440c5eaa829efb6f9967aaad89022acbe644c39b36",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x0313ad57fdf765addc71329abf8d74ac2bce6d46da8c2b9b82255a5076620301",
                    ),
                    str_to_felt_bytes(
                        "0x0453ae0c9610197b18b13645c44d3d0a407083d96562e8752aab3fab616cecb0",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x05aee31408163292105d875070f98cb48275b8c87e80380b78d30647e05854d5",
                    ),
                    str_to_felt_bytes(
                        "0x00000000000000000000000000000000000000000000000000000000000007e5",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x06cf6c2f36d36b08e591e4489e92ca882bb67b9c39a3afccf011972a8de467f0",
                    ),
                    str_to_felt_bytes(
                        "0x07ab344d88124307c07b56f6c59c12f4543e9c96398727854a322dea82c73240",
                    ),
                ),
            ],
        ),
        (
            str_to_felt_bytes("0x031c887d82502ceb218c06ebb46198da3f7b92864a8223746bc836dda3e34b52"),
            vec![
                (
                    str_to_felt_bytes(
                        "0x00df28e613c065616a2e79ca72f9c1908e17b8c913972a9993da77588dc9cae9",
                    ),
                    str_to_felt_bytes(
                        "0x01432126ac23c7028200e443169c2286f99cdb5a7bf22e607bcd724efa059040",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x05f750dc13ed239fa6fc43ff6e10ae9125a33bd05ec034fc3bb4dd168df3505f",
                    ),
                    str_to_felt_bytes(
                        "0x00000000000000000000000000000000000000000000000000000000000007c7",
                    ),
                ),
            ],
        ),
    ];

    let block_1 = [
        (
            str_to_felt_bytes("0x06538fdd3aa353af8a87f5fe77d1f533ea82815076e30a86d65b72d3eb4f0b80"),
            vec![
                (
                    str_to_felt_bytes(
                        "0x0000000000000000000000000000000000000000000000000000000000000005",
                    ),
                    str_to_felt_bytes(
                        "0x000000000000000000000000000000000000000000000000000000000000022b",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x01aed933fd362faecd8ea54ee749092bd21f89901b7d1872312584ac5b636c6d",
                    ),
                    str_to_felt_bytes(
                        "0x00000000000000000000000000000000000000000000000000000000000007e5",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x010212fa2be788e5d943714d6a9eac5e07d8b4b48ead96b8d0a0cbe7a6dc3832",
                    ),
                    str_to_felt_bytes(
                        "0x008a81230a7e3ffa40abe541786a9b69fbb601434cec9536d5d5b2ee4df90383",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x00ffda4b5cf0dce9bc9b0d035210590c73375fdbb70cd94ec6949378bffc410c",
                    ),
                    str_to_felt_bytes(
                        "0x02b36318931915f71777f7e59246ecab3189db48408952cefda72f4b7977be51",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x00ffda4b5cf0dce9bc9b0d035210590c73375fdbb70cd94ec6949378bffc410d",
                    ),
                    str_to_felt_bytes(
                        "0x07e928dcf189b05e4a3dae0bc2cb98e447f1843f7debbbf574151eb67cda8797",
                    ),
                ),
            ],
        ),
        (
            str_to_felt_bytes("0x0327d34747122d7a40f4670265b098757270a449ec80c4871450fffdab7c2fa8"),
            vec![
                (
                    str_to_felt_bytes(
                        "0x0000000000000000000000000000000000000000000000000000000000000005",
                    ),
                    str_to_felt_bytes(
                        "0x0000000000000000000000000000000000000000000000000000000000000065",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x01aed933fd362faecd8ea54ee749092bd21f89901b7d1872312584ac5b636c6d",
                    ),
                    str_to_felt_bytes(
                        "0x00000000000000000000000000000000000000000000000000000000000007c7",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x04184fa5a6d40f47a127b046ed6facfa3e6bc3437b393da65cc74afe47ca6c6e",
                    ),
                    str_to_felt_bytes(
                        "0x001ef78e458502cd457745885204a4ae89f3880ec24db2d8ca97979dce15fedc",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x05591c8c3c8d154a30869b463421cd5933770a0241e1a6e8ebcbd91bdd69bec4",
                    ),
                    str_to_felt_bytes(
                        "0x026b5943d4a0c420607cee8030a8cdd859bf2814a06633d165820960a42c6aed",
                    ),
                ),
                (
                    str_to_felt_bytes(
                        "0x05591c8c3c8d154a30869b463421cd5933770a0241e1a6e8ebcbd91bdd69bec5",
                    ),
                    str_to_felt_bytes(
                        "0x01518eec76afd5397cefd14eda48d01ad59981f9ce9e70c233ca67acd8754008",
                    ),
                ),
            ],
        ),
    ];

    let block_2 = vec![
        (
            str_to_felt_bytes("0x001fb4457f3fe8a976bdb9c04dd21549beeeb87d3867b10effe0c4bd4064a8e4"),
            vec![(
                str_to_felt_bytes(
                    "0x056c060e7902b3d4ec5a327f1c6e083497e586937db00af37fe803025955678f",
                ),
                str_to_felt_bytes(
                    "0x075495b43f53bd4b9c9179db113626af7b335be5744d68c6552e3d36a16a747c",
                ),
            )],
        ),
        (
            str_to_felt_bytes("0x05790719f16afe1450b67a92461db7d0e36298d6a5f8bab4f7fd282050e02f4f"),
            vec![(
                str_to_felt_bytes(
                    "0x0772c29fae85f8321bb38c9c3f6edb0957379abedc75c17f32bcef4e9657911a",
                ),
                str_to_felt_bytes(
                    "0x06d4ca0f72b553f5338a95625782a939a49b98f82f449c20f49b42ec60ed891c",
                ),
            )],
        ),
        (
            str_to_felt_bytes("0x057b973bf2eb26ebb28af5d6184b4a044b24a8dcbf724feb95782c4d1aef1ca9"),
            vec![(
                str_to_felt_bytes(
                    "0x04f2c206f3f2f1380beeb9fe4302900701e1cb48b9b33cbe1a84a175d7ce8b50",
                ),
                str_to_felt_bytes(
                    "0x02a614ae71faa2bcdacc5fd66965429c57c4520e38ebc6344f7cf2e78b21bd2f",
                ),
            )],
        ),
        (
            str_to_felt_bytes("0x02d6c9569dea5f18628f1ef7c15978ee3093d2d3eec3b893aac08004e678ead3"),
            vec![(
                str_to_felt_bytes(
                    "0x07f93985c1baa5bd9b2200dd2151821bd90abb87186d0be295d7d4b9bc8ca41f",
                ),
                str_to_felt_bytes(
                    "0x0127cd00a078199381403a33d315061123ce246c8e5f19aa7f66391a9d3bf7c6",
                ),
            )],
        ),
    ];

    let blocks = block_0.iter().chain(block_1.iter()).chain(block_2.iter());

    // Inserts all storage updates into the bonsai
    for (contract_address, storage) in blocks.clone() {
        log::info!(
            "contract address (write): {:#064x}",
            Felt::from_bytes_be_slice(contract_address)
        );

        for (k, v) in storage {
            // truncate only keeps the first 251 bits in a key
            // so there should be no error during insertion
            let ktrunc = &truncate(k);
            let kfelt0 = Felt::from_bytes_be_slice(k);
            let kfelt1 = Felt::from_bytes_be_slice(ktrunc.as_raw_slice());

            // quick sanity check to make sure truncating a key does not remove any data
            assert_eq!(kfelt0, kfelt1);

            let v = &Felt::from_bytes_be_slice(v);
            assert!(bonsai.insert(contract_address, ktrunc, v).is_ok());
        }
    }
    assert!(bonsai.commit(BasicId::new(0)).is_ok());

    // aggreates all storage changes to their latest state
    // (replacements are takent into account)
    let mut storage_map = IndexMap::<ByteVec, IndexMap<Felt, Felt>>::new();
    for (contract_address, storage) in blocks.clone() {
        let map = storage_map.entry((*contract_address).into()).or_default();

        for (k, v) in storage {
            let k = Felt::from_bytes_be_slice(k);
            let v = Felt::from_bytes_be_slice(v);
            map.insert(k, v);
        }
    }

    // checks for each contract if the original key can be reconstructed
    // from the data stored in the db
    for (contract_address, storage) in storage_map.iter() {
        log::info!(
            "contract address (read): {:#064x}",
            Felt::from_bytes_be_slice(contract_address)
        );

        let keys = bonsai.get_keys(contract_address).unwrap();
        log::debug!("{keys:?}");
        for k in keys {
            // if all has gone well, the db should contain the first 251 bits of the key,
            // which should represent the entirety of the data
            let k = Felt::from_bytes_be_slice(&k);
            log::info!("looking for key: {k:#064x}");

            assert!(storage.contains_key(&k));
        }
    }

    // makes sure retrieving key-value pairs works for each contract
    for (contract_address, storage) in storage_map.iter() {
        log::info!(
            "contract address (read): {:#064x}",
            Felt::from_bytes_be_slice(contract_address)
        );

        let kv = bonsai.get_key_value_pairs(contract_address).unwrap();
        log::debug!("{kv:?}");
        for (k, v) in kv {
            let k = Felt::from_bytes_be_slice(&k);
            let v = Felt::from_bytes_be_slice(&v);
            log::info!("checking for key-value pair:({k:#064x}, {v:#064x})");

            assert_eq!(*storage.get(&k).unwrap(), v);
        }
    }
}

fn str_to_felt_bytes(hex: &str) -> [u8; 32] {
    Felt::from_hex(hex).unwrap().to_bytes_be()
}

fn truncate(key: &[u8]) -> BitVec {
    key.view_bits()[5..].to_owned()
}

// use crate::{
//     databases::{create_rocks_db, RocksDB, RocksDBConfig},
//     id::BasicId,
//     key_value_db::KeyValueDBConfig,
//     KeyValueDB,
// };
// use mp_felt::Felt252Wrapper;
// use mp_hashers::pedersen::PedersenHasher;
// use parity_scale_codec::{Decode, Encode};
// use rand::prelude::*;
// use starknet_types_core::{felt::Felt, hash::Pedersen};

// // convert a Madara felt to a standard Felt
// fn felt_from_madara_felt(madara_felt: &Felt252Wrapper) -> Felt {
//     let encoded = madara_felt.encode();
//     Felt::decode(&mut &encoded[..]).unwrap()
// }

// // convert a standard Felt to a Madara felt
// fn madara_felt_from_felt(felt: &Felt) -> Felt252Wrapper {
//     let encoded = felt.encode();
//     Felt252Wrapper::decode(&mut &encoded[..]).unwrap()
// }

// #[test]
// fn one_commit_tree_compare() {
//     let mut elements = vec![];
//     let tempdir = tempfile::tempdir().unwrap();
//     let mut rng = rand::thread_rng();
//     let tree_size = rng.gen_range(10..100);
//     for _ in 0..tree_size {
//         let mut element = String::from("0x");
//         let element_size = rng.gen_range(10..32);
//         for _ in 0..element_size {
//             let random_byte: u8 = rng.gen();
//             element.push_str(&format!("{:02x}", random_byte));
//         }
//         elements.push(Felt::from_hex(&element).unwrap());
//     }
//     let madara_elements = elements
//         .iter()
//         .map(madara_felt_from_felt)
//         .collect::<Vec<_>>();
//     let rocks_db = create_rocks_db(std::path::Path::new(tempdir.path())).unwrap();
//     let rocks_db = RocksDB::new(&rocks_db, RocksDBConfig::default());
//     let db = KeyValueDB::new(rocks_db, KeyValueDBConfig::default(), None);
//     let mut bonsai_tree: super::MerkleTree<Pedersen, RocksDB<BasicId>, BasicId> =
//         super::MerkleTree::new(db).unwrap();
//     let root_hash = mp_commitments::calculate_class_commitment_tree_root_hash::<PedersenHasher>(
//         &madara_elements,
//     );
//     elements
//         .iter()
//         .zip(madara_elements.iter())
//         .for_each(|(element, madara_element)| {
//             let final_hash =
//                 calculate_class_commitment_leaf_hash::<PedersenHasher>(*madara_element);
//             let key = &element.to_bytes_be()[..31];
//             bonsai_tree
//                 .set(
//                     &BitVec::from_vec(key.to_vec()),
//                     felt_from_madara_felt(&final_hash),
//                 )
//                 .unwrap();
//         });
//     bonsai_tree.display();
//     assert_eq!(
//         bonsai_tree.commit().unwrap(),
//         felt_from_madara_felt(&root_hash)
//     );
// }

// #[test]
// fn simple_commits() {
//     let tempdir = tempfile::tempdir().unwrap();
//     let mut madara_tree = StateCommitmentTree::<PedersenHasher>::default();
//     let rocks_db = create_rocks_db(std::path::Path::new(tempdir.path())).unwrap();
//     let rocks_db = RocksDB::new(&rocks_db, RocksDBConfig::default());
//     let db = KeyValueDB::new(rocks_db, KeyValueDBConfig::default(), None);
//     let mut bonsai_tree: super::MerkleTree<Pedersen, RocksDB<BasicId>, BasicId> =
//         super::MerkleTree::new(db).unwrap();
//     let elements = [
//         [Felt::from_hex("0x665342762FDD54D0303c195fec3ce2568b62052e").unwrap()],
//         [Felt::from_hex("0x66342762FDD54D0303c195fec3ce2568b62052e").unwrap()],
//         [Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap()],
//     ];
//     for elem in elements {
//         elem.iter().for_each(|class_hash| {
//             let final_hash =
//                 felt_from_madara_felt(&calculate_class_commitment_leaf_hash::<PedersenHasher>(
//                     madara_felt_from_felt(class_hash),
//                 ));
//             madara_tree.set(
//                 madara_felt_from_felt(class_hash),
//                 madara_felt_from_felt(&final_hash),
//             );
//             let key = &class_hash.to_bytes_be()[..31];
//             bonsai_tree
//                 .set(&BitVec::from_vec(key.to_vec()), final_hash)
//                 .unwrap();
//         });
//     }
//     let madara_root_hash = madara_tree.commit();
//     let bonsai_root_hash = bonsai_tree.commit().unwrap();
//     assert_eq!(bonsai_root_hash, felt_from_madara_felt(&madara_root_hash));
// }

// #[test]
// fn simple_commits_and_delete() {
//     let tempdir = tempfile::tempdir().unwrap();
//     let rocks_db = create_rocks_db(std::path::Path::new(tempdir.path())).unwrap();
//     let rocks_db = RocksDB::new(&rocks_db, RocksDBConfig::default());
//     let db = KeyValueDB::new(rocks_db, KeyValueDBConfig::default(), None);
//     let mut bonsai_tree: super::MerkleTree<Pedersen, RocksDB<BasicId>, BasicId> =
//         super::MerkleTree::new(db).unwrap();
//     let elements = [
//         [Felt::from_hex("0x665342762FDD54D0303c195fec3ce2568b62052e").unwrap()],
//         [Felt::from_hex("0x66342762FDD54D0303c195fec3ce2568b62052e").unwrap()],
//         [Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap()],
//     ];
//     for elem in elements {
//         elem.iter().for_each(|class_hash| {
//             let final_hash = calculate_class_commitment_leaf_hash::<PedersenHasher>(
//                 madara_felt_from_felt(class_hash),
//             );
//             let key = &class_hash.to_bytes_be()[..31];
//             bonsai_tree
//                 .set(
//                     &BitVec::from_vec(key.to_vec()),
//                     felt_from_madara_felt(&final_hash),
//                 )
//                 .unwrap();
//         });
//     }
//     bonsai_tree.commit().unwrap();
//     for elem in elements {
//         elem.iter().for_each(|class_hash| {
//             let key = &class_hash.to_bytes_be()[..31];
//             bonsai_tree
//                 .set(&BitVec::from_vec(key.to_vec()), Felt::ZERO)
//                 .unwrap();
//         });
//     }
//     bonsai_tree.commit().unwrap();
// }

// #[test]
// fn multiple_commits_tree_compare() {
//     let mut rng = rand::thread_rng();
//     let tempdir = tempfile::tempdir().unwrap();
//     let mut madara_tree = StateCommitmentTree::<PedersenHasher>::default();
//     let rocks_db = create_rocks_db(std::path::Path::new(tempdir.path())).unwrap();
//     let rocks_db = RocksDB::new(&rocks_db, RocksDBConfig::default());
//     let db = KeyValueDB::new(rocks_db, KeyValueDBConfig::default(), None);
//     let mut bonsai_tree: super::MerkleTree<Pedersen, RocksDB<BasicId>, BasicId> =
//         super::MerkleTree::new(db).unwrap();
//     let nb_commits = rng.gen_range(2..4);
//     for _ in 0..nb_commits {
//         let mut elements = vec![];
//         let tree_size = rng.gen_range(10..100);
//         for _ in 0..tree_size {
//             let mut element = String::from("0x");
//             let element_size = rng.gen_range(10..32);
//             for _ in 0..element_size {
//                 let random_byte: u8 = rng.gen();
//                 element.push_str(&format!("{:02x}", random_byte));
//             }
//             elements.push(Felt::from_hex(&element).unwrap());
//         }
//         elements.iter().for_each(|class_hash| {
//             let final_hash = calculate_class_commitment_leaf_hash::<PedersenHasher>(
//                 madara_felt_from_felt(class_hash),
//             );
//             madara_tree.set(madara_felt_from_felt(class_hash), final_hash);
//             let key = &class_hash.to_bytes_be()[..31];
//             bonsai_tree
//                 .set(
//                     &BitVec::from_vec(key.to_vec()),
//                     felt_from_madara_felt(&final_hash),
//                 )
//                 .unwrap();
//         });

//         let bonsai_root_hash = bonsai_tree.commit().unwrap();
//         let madara_root_hash = madara_tree.commit();
//         assert_eq!(bonsai_root_hash, felt_from_madara_felt(&madara_root_hash));
//     }
// }

// #[test]    // fn multiple_commits_tree_compare_with_deletes() {
//     let mut rng = rand::thread_rng();
//     let mut madara_tree = StateCommitmentTree::<PedersenHasher>::default();
//     let rocks_db = create_rocks_db(std::path::Path::new("test_db")).unwrap();
//     let mut db = RocksDB::new(&rocks_db, RocksDBConfig::default());
//     let mut bonsai_tree: super::MerkleTree<PedersenHasher, RocksDB> =
//         super::MerkleTree::empty(&mut db);
//     let nb_commits = rng.gen_range(2..5);
//     let mut elements_to_delete = vec![];
//     for _ in 0..nb_commits {
//         let mut elements = vec![];
//         let tree_size = rng.gen_range(10..100);
//         for _ in 0..tree_size {
//             let mut element = String::from("0x");
//             let element_size = rng.gen_range(10..32);
//             for _ in 0..element_size {
//                 let random_byte: u8 = rng.gen();
//                 element.push_str(&format!("{:02x}", random_byte));
//             }
//             if rng.gen_bool(0.1) {
//                 elements_to_delete.push(Felt::from_hex_be(&element).unwrap());
//                 elements.push(Felt::from_hex_be(&element).unwrap());
//             } else {
//                 elements.push(Felt::from_hex_be(&element).unwrap());
//             }
//         }
//         elements.iter().for_each(|class_hash| {
//             let final_hash =
//                 calculate_class_commitment_leaf_hash::<PedersenHasher>(*class_hash);
//             madara_tree.set(*class_hash, final_hash);
//             let key = &class_hash.0.to_bytes_be()[..31];
//             bonsai_tree.set(&BitVec::from_vec(key.to_vec()), final_hash);
//         });

//         let bonsai_root_hash = bonsai_tree.commit();
//         let madara_root_hash = madara_tree.commit();
//         assert_eq!(bonsai_root_hash, madara_root_hash);
//     }
//     elements_to_delete.iter().for_each(|class_hash| {
//         madara_tree.set(*class_hash, Felt::ZERO);
//         let key = &class_hash.0.to_bytes_be()[..31];
//         bonsai_tree.set(&BitVec::from_vec(key.to_vec()), Felt::ZERO);
//     });

//     let bonsai_root_hash = bonsai_tree.commit();
//     let madara_root_hash = madara_tree.commit();
//     assert_eq!(bonsai_root_hash, madara_root_hash);
// }

// #[test]
// fn test_proof() {
//     let tempdir = tempfile::tempdir().unwrap();
//     let rocks_db = create_rocks_db(std::path::Path::new(tempdir.path())).unwrap();
//     let rocks_db = RocksDB::new(&rocks_db, RocksDBConfig::default());
//     let db = KeyValueDB::new(rocks_db, KeyValueDBConfig::default(), None);
//     let mut bonsai_tree: super::MerkleTree<Pedersen, RocksDB<BasicId>, BasicId> =
//         super::MerkleTree::new(db).unwrap();
//     let elements = [
//         [Felt252Wrapper::from_hex_be("0x665342762FDD54D0303c195fec3ce2568b62052e").unwrap()],
//         [Felt252Wrapper::from_hex_be("0x66342762FDD54D0303c195fec3ce2568b62052e").unwrap()],
//         [Felt252Wrapper::from_hex_be("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap()],
//     ];
//     for elem in elements {
//         elem.iter().for_each(|class_hash| {
//             let final_hash =
//                 calculate_class_commitment_leaf_hash::<PedersenHasher>(*class_hash);
//             let key = &class_hash.0.to_bytes_be()[..31];
//             bonsai_tree
//                 .set(
//                     &BitVec::from_vec(key.to_vec()),
//                     Felt::from_bytes_be(&final_hash.0.to_bytes_be()),
//                 )
//                 .unwrap();
//         });
//     }
//     bonsai_tree.commit().unwrap();
//     let bonsai_proof = bonsai_tree
//         .get_proof(&BitVec::from_vec(
//             elements[0][0].0.to_bytes_be()[..31].to_vec(),
//         ))
//         .unwrap();
//     println!("bonsai_proof: {:?}", bonsai_proof);
// }

// test in madara
//     #[test]
// fn test_proof() {
//     let mut tree = super::merkle_patricia_tree::merkle_tree::MerkleTree::<PedersenHasher>::empty();
//     let elements = [
//         [Felt252Wrapper::from_hex_be("0x665342762FDD54D0303c195fec3ce2568b62052e").unwrap()],
//         [Felt252Wrapper::from_hex_be("0x66342762FDD54D0303c195fec3ce2568b62052e").unwrap()],
//         [Felt252Wrapper::from_hex_be("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap()],
//     ];
//     for elem in elements {
//         elem.iter().for_each(|class_hash| {
//             let final_hash =
//                 calculate_class_commitment_leaf_hash::<PedersenHasher>(*class_hash);
//             let key = &class_hash.0.to_bytes_be()[..31];
//             tree
//                 .set(&BitVec::from_vec(key.to_vec()), final_hash)
//         });
//     }
//     tree.commit();
//     let bonsai_proof = tree.get_proof(&BitVec::from_vec(
//         elements[0][0].0.to_bytes_be()[..31].to_vec(),
//     ));
//     println!("bonsai_proof: {:?}", bonsai_proof);
// }
