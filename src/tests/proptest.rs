#![cfg(feature = "std")]

use crate::MerkleTree;

use core::fmt::{self, Debug};

use crate::databases::HashMapDb;
use crate::id::BasicId;
use crate::key_value_db::KeyValueDB;
use crate::HashMap;

use bitvec::bitvec;
use bitvec::order::Msb0;
use bitvec::vec::BitVec;
use proptest::prelude::*;
use proptest_derive::Arbitrary;
use smallvec::smallvec;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Pedersen;

#[derive(PartialEq, Eq, Hash)]
struct Key(BitVec<u8, Msb0>);
impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:b}", self.0)
    }
}
impl Arbitrary for Key {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        <[bool; 5]>::arbitrary()
            .prop_map(|arr| arr.into_iter().collect::<BitVec<u8, Msb0>>())
            .prop_map(Self)
            .boxed()
    }
}

#[derive(PartialEq, Eq, Hash)]
struct Value(Felt);
impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}
impl Arbitrary for Value {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        <[bool; 251]>::arbitrary()
            .prop_map(|arr| arr.into_iter().collect::<BitVec<u8, Msb0>>())
            .prop_map(|vec| Felt::from_bytes_be(vec.as_raw_slice().try_into().unwrap()))
            .prop_map(Self)
            .boxed()
    }
}

#[derive(Debug, Arbitrary)]
enum Step {
    Insert(Key, Value),
    Remove(Key),
    Commit,
}

#[derive(Debug, Arbitrary)]
struct MerkleTreeInsertProblem(Vec<Step>);

impl MerkleTreeInsertProblem {
    fn check(&self) {
        let mut hashmap_db = KeyValueDB::<_, BasicId>::new(
            HashMapDb::<BasicId>::default(),
            Default::default(),
            None,
        );

        let mut ckv = HashMap::new();

        // apply steps
        let mut tree = MerkleTree::<Pedersen>::new(smallvec![]);
        for step in &self.0 {
            match step {
                Step::Insert(k, v) => {
                    log::trace!("== STEP == setting {k:?} => {v:?}");
                    ckv.insert(k.0.clone(), v.0);
                    tree.set(&hashmap_db, &k.0, v.0).unwrap();
                }
                Step::Remove(k) => {
                    log::trace!("== STEP == removing {k:?}");
                    ckv.insert(k.0.clone(), Felt::ZERO);
                    tree.set(&hashmap_db, &k.0, Felt::ZERO).unwrap();
                }
                Step::Commit => {
                    log::trace!("== STEP == commit");
                    tree.commit(&mut hashmap_db).unwrap();
                }
            }
            log::trace!("TREE");
            tree.display();
        }

        // check
        for (k, v) in &ckv {
            log::trace!("checking {k:b}.....");
            let v2 = tree.get(&hashmap_db, k).unwrap().unwrap_or_default();
            log::trace!("checking that {k:b} => {v:#x}, (tree returned {v2:#x})");
            assert_eq!(Value(*v), Value(v2))
        }

        // check for leaks
        for (k, _v) in ckv {
            log::trace!("removing {k:b}..... (check for leaks)");
            tree.set(&hashmap_db, &k, Felt::ZERO).unwrap();
            tree.commit(&mut hashmap_db).unwrap();
        }

        hashmap_db.db.assert_empty();
        tree.assert_empty();

        log::trace!("okay!");
        log::trace!("");
        log::trace!("");
        log::trace!("");
    }
}

proptest::proptest! {
    #[test]
    fn proptest_inserts(pb in any::<MerkleTreeInsertProblem>()) {
        let _ = env_logger::builder().is_test(true).try_init();
        log::set_max_level(log::LevelFilter::Trace);
        pb.check();
    }
}

#[test]
fn test_merkle_pb_1() {
    use Step::*;
    let _ = env_logger::builder().is_test(true).try_init();
    log::set_max_level(log::LevelFilter::Trace);
    let pb = MerkleTreeInsertProblem(vec![
        Insert(
            Key(bitvec![u8, Msb0; 1,0,0,1,1]),
            Value(Felt::from_hex("0x20").unwrap()),
        ),
        Remove(Key(bitvec![u8, Msb0; 1,0,0,1,1])),
        Remove(Key(bitvec![u8, Msb0; 0,0,0,0,0])),
        Insert(
            Key(bitvec![u8, Msb0; 0,0,0,0,0]),
            Value(Felt::from_hex("0x20").unwrap()),
        ),
        Commit,
        Remove(Key(bitvec![u8, Msb0; 0,0,0,0,0])),
    ]);

    pb.check();
}

#[test]
fn test_merkle_pb_2() {
    use Step::*;
    let _ = env_logger::builder().is_test(true).try_init();
    log::set_max_level(log::LevelFilter::Trace);
    let pb = MerkleTreeInsertProblem(vec![
        Insert(
            Key(bitvec![u8, Msb0; 0,1,0,0,0]),
            Value(Felt::from_hex("0x20").unwrap()),
        ),
        // Remove(
        //     Key(bitvec![u8, Msb0; 0,0,0,0,0]),
        // ),
        Insert(
            Key(bitvec![u8, Msb0; 0,0,0,0,0]),
            Value(Felt::from_hex("0x80").unwrap()),
        ),
        Remove(Key(bitvec![u8, Msb0; 0,0,0,0,0])),
        Commit,
    ]);

    pb.check();
}

#[test]
fn test_merkle_pb_3() {
    use Step::*;
    let _ = env_logger::builder().is_test(true).try_init();
    log::set_max_level(log::LevelFilter::Trace);
    let pb = MerkleTreeInsertProblem(vec![
        Insert(
            Key(bitvec![u8, Msb0; 1,0,0,0,0]),
            Value(Felt::from_hex("0x21").unwrap()),
        ),
        Insert(
            Key(bitvec![u8, Msb0; 1,1,0,0,0]),
            Value(Felt::from_hex("0x22").unwrap()),
        ),
        Insert(
            Key(bitvec![u8, Msb0; 1,1,0,1,0]),
            Value(Felt::from_hex("0x23").unwrap()),
        ),
        Remove(Key(bitvec![u8, Msb0; 1,0,0,0,0])),
        Remove(Key(bitvec![u8, Msb0; 1,0,0,0,0])),
        Commit,
    ]);

    pb.check();
}

#[test]
fn test_merkle_pb_4() {
    use Step::*;
    let _ = env_logger::builder().is_test(true).try_init();
    log::set_max_level(log::LevelFilter::Trace);
    let pb = MerkleTreeInsertProblem(vec![
        // Remove(
        //     Key(bitvec![u8, Msb0; 0,0,0,0,0]),
        // ),
        Insert(
            Key(bitvec![u8, Msb0; 0,0,1,0,0]),
            Value(Felt::from_hex("0x20").unwrap()),
        ),
        Commit,
        Insert(
            Key(bitvec![u8, Msb0; 0,0,0,0,0]),
            Value(Felt::from_hex("0x21").unwrap()),
        ),
        Commit,
    ]);

    pb.check();
}

#[test]
fn test_merkle_pb_5() {
    use Step::*;
    let _ = env_logger::builder().is_test(true).try_init();
    log::set_max_level(log::LevelFilter::Trace);
    let pb = MerkleTreeInsertProblem(vec![
        Insert(
            Key(bitvec![u8, Msb0; 0,0,0,0,1]),
            Value(Felt::from_hex("0x20").unwrap()),
        ),
        Insert(
            Key(bitvec![u8, Msb0; 0,0,1,0,0]),
            Value(Felt::from_hex("0x20").unwrap()),
        ),
    ]);

    pb.check();
}

#[test]
fn test_merkle_pb_6() {
    use Step::*;
    let _ = env_logger::builder().is_test(true).try_init();
    log::set_max_level(log::LevelFilter::Trace);
    let pb = MerkleTreeInsertProblem(vec![
        Insert(
            Key(bitvec![u8, Msb0; 1,0,0,0,0]),
            Value(Felt::from_hex("0x20").unwrap()),
        ),
        Insert(
            Key(bitvec![u8, Msb0; 1,1,0,0,0]),
            Value(Felt::from_hex("0x20").unwrap()),
        ),
        Commit,
        Insert(
            Key(bitvec![u8, Msb0; 1,1,0,1,0]),
            Value(Felt::from_hex("0x20").unwrap()),
        ),
        Insert(
            Key(bitvec![u8, Msb0; 1,0,0,0,0]),
            Value(Felt::from_hex("0x20").unwrap()),
        ),
        Remove(Key(bitvec![u8, Msb0; 1,0,0,0,0])),
        Remove(Key(bitvec![u8, Msb0; 1,0,0,0,0])),
    ]);

    pb.check();
}
