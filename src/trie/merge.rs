use starknet_types_core::hash::StarkHash;
use std::mem;

use crate::{
    hash_map,
    trie::{
        merkle_node::{BinaryNode, Direction, EdgeNode, Node, NodeHandle, NodeId},
        merkle_tree::{NodesMapping, RootHandle},
        path::Path,
    },
    BonsaiDatabase, BonsaiStorageError, MerkleTree,
};

impl<H: StarkHash + Send + Sync> MerkleTree<H> {
    pub(crate) fn merge<DB: BonsaiDatabase>(
        &mut self,
        mut other: MerkleTree<H>,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        fn copy_handle<DB: BonsaiDatabase>(
            a_map: &mut NodesMapping,
            a_next_id: &mut NodeId,
            b_map: &mut NodesMapping,
            handle: NodeHandle,
        ) -> Result<NodeHandle, BonsaiStorageError<DB::DatabaseError>> {
            let id = a_next_id.next_id();

            match handle {
                NodeHandle::Hash(felt) => Ok(NodeHandle::Hash(felt)),
                NodeHandle::InMemory(b_subtree) => {
                    copy_subtree::<DB>(a_map, a_next_id, b_map, id, b_subtree)
                }
            }
        }

        fn copy_subtree<DB: BonsaiDatabase>(
            a_map: &mut NodesMapping,
            a_next_id: &mut NodeId,
            b_map: &mut NodesMapping,
            a_id: NodeId,
            b_subtree: NodeId,
        ) -> Result<NodeHandle, BonsaiStorageError<DB::DatabaseError>> {
            let b = b_map.0.remove(&b_subtree).ok_or_else(|| {
                BonsaiStorageError::Trie("node id has no associated node in storage".into())
            })?;

            let new_node = match b {
                Node::Binary(b) => {
                    let left = copy_handle::<DB>(a_map, a_next_id, b_map, b.left)?;
                    let right = copy_handle::<DB>(a_map, a_next_id, b_map, b.right)?;
                    Node::Binary(BinaryNode {
                        hash: None,
                        height: 0,
                        left,
                        right,
                    })
                }
                Node::Edge(b) => {
                    let child = copy_handle::<DB>(a_map, a_next_id, b_map, b.child)?;
                    Node::Edge(EdgeNode {
                        hash: None,
                        height: 0,
                        path: b.path,
                        child: child,
                    })
                }
            };

            a_map.0.insert(a_id, new_node);

            Ok(NodeHandle::InMemory(a_id))
        }

        #[derive(Debug)]
        struct PendingSubtreeCopy {
            a_id: NodeId,
            b_subtree: NodeId,
        }

        fn merge_handles<DB: BonsaiDatabase>(
            a: &mut NodeHandle,
            a_next_id: &mut NodeId,
            b: NodeHandle,
            pending_copy: &mut Vec<PendingSubtreeCopy>,
            visit_next: &mut Vec<(NodeId, NodeId)>,
        ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
            match (a, b) {
                (_, NodeHandle::Hash(_)) => {}
                (a @ NodeHandle::Hash(_), NodeHandle::InMemory(b_subtree)) => {
                    let a_id = a_next_id.next_id();
                    *a = NodeHandle::InMemory(a_id);

                    pending_copy.push(PendingSubtreeCopy { a_id, b_subtree });
                }
                (NodeHandle::InMemory(a), NodeHandle::InMemory(b)) => visit_next.push((*a, b)),
            }

            Ok(())
        }

        fn merge_nodeid<DB: BonsaiDatabase>(
            nodeid_a: NodeId,
            a_map: &mut NodesMapping,
            a_next_id: &mut NodeId,
            nodeid_b: NodeId,
            b_map: &mut NodesMapping,
            pending_copy: &mut Vec<PendingSubtreeCopy>,
            pending_insertion: &mut Vec<(NodeId, Node)>,
            visit_next: &mut Vec<(NodeId, NodeId)>,
        ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
            // get the two nodes as mut

            let a = a_map.0.get_mut(&nodeid_a).ok_or_else(|| {
                BonsaiStorageError::Trie("node id has no associated node in storage".into())
            })?;
            let mut b = match b_map.0.entry(nodeid_b) {
                hash_map::Entry::Occupied(entry) => entry,
                hash_map::Entry::Vacant(_) => {
                    return Err(BonsaiStorageError::Trie(
                        "node id has no associated node in storage".into(),
                    ))
                }
            };

            log::trace!("Current step: nodeid_a={nodeid_a:?} {a:?}, nodeid_b={nodeid_b:?} {b:?}");

            // this step does not modify a_map or b_map, as we still have mut reference inside them
            // instead, we collect instruction about what to add / copy / visit, drop the mut ref to a and b nodes, and then apply them

            let mut remove_b = false;

            let new_a = match (a.clone(), b.get_mut()) {
                // Binary and binary: go down both arms
                (Node::Binary(mut a), Node::Binary(b)) => {
                    merge_handles::<DB>(&mut a.left, a_next_id, b.left, pending_copy, visit_next)?;
                    merge_handles::<DB>(
                        &mut a.right,
                        a_next_id,
                        b.right,
                        pending_copy,
                        visit_next,
                    )?;
                    remove_b = true;

                    Node::Binary(a)
                }

                // Binary and edge
                (Node::Binary(mut a), Node::Edge(b)) => {
                    // remove leading bit
                    let removed_bit = *b.path.0.get(0).ok_or_else(|| {
                        BonsaiStorageError::Trie("storage has an edge with an empty path".into())
                    })?;

                    b.path.0.drain(0..1);

                    // merge the binary node child with the edge
                    let a_h = a.get_child_mut(Direction::from(removed_bit));
                    let b = if b.path.0.is_empty() {
                        // use child instead
                        remove_b = true;
                        b.child
                    } else {
                        NodeHandle::InMemory(nodeid_b)
                    };
                    merge_handles::<DB>(a_h, a_next_id, b, pending_copy, visit_next)?;

                    Node::Binary(a)
                }

                // Edge and binary
                // a) |   b) |
                //   [ ]     o
                //    |     / \
                // 1) We pop the first bit of the edge
                // 2) Replace the a) node with a binary
                // 3) Depending on the bit, one of the children is a subtree copy from b), the other one is merge a) and other child from b)
                (
                    Node::Edge(EdgeNode {
                        mut path, child, ..
                    }),
                    Node::Binary(b),
                ) => {
                    let mut path = mem::take(&mut path.0);
                    let removed_bit = *path.get(0).ok_or_else(|| {
                        BonsaiStorageError::Trie("storage has an edge with an empty path".into())
                    })?;
                    path.drain(0..1);

                    let child = child.clone();

                    let b_subtree_to_merge = b.get_child(Direction::from(removed_bit));
                    let b_subtree_to_copy = b.get_child(Direction::from(!removed_bit));

                    let mut a_node_to_merge = if path.is_empty() {
                        // use child instead
                        child
                    } else {
                        let new_node = a_next_id.next_id();
                        pending_insertion.push((
                            new_node,
                            Node::Edge(EdgeNode {
                                hash: None,
                                height: 0,
                                path: Path(path),
                                child,
                            }),
                        ));

                        NodeHandle::InMemory(new_node)
                    };

                    merge_handles::<DB>(
                        &mut a_node_to_merge,
                        a_next_id,
                        b_subtree_to_merge,
                        pending_copy,
                        visit_next,
                    )?;

                    let a_copied_subtree = match b_subtree_to_copy {
                        NodeHandle::Hash(felt) => NodeHandle::Hash(felt),
                        NodeHandle::InMemory(b_subtree) => {
                            let a_id = a_next_id.next_id();
                            pending_copy.push(PendingSubtreeCopy { a_id, b_subtree });
                            NodeHandle::InMemory(a_id)
                        }
                    };

                    let (left, right) = if removed_bit {
                        // copy left, merge right
                        (a_copied_subtree, a_node_to_merge)
                    } else {
                        (a_node_to_merge, a_copied_subtree)
                    };

                    remove_b = true;

                    Node::Binary(BinaryNode {
                        hash: None,
                        height: 0,
                        left,
                        right,
                    })
                }

                // Edge edge
                (Node::Edge(mut a_node), Node::Edge(b_node)) => {
                    // find the matching prefix
                    let common = a_node.common_path(&b_node.path.0);

                    if common.len() == a_node.path.0.len() {
                        // edge is the same, go to child
                        merge_handles::<DB>(
                            &mut a_node.child,
                            a_next_id,
                            b_node.child,
                            pending_copy,
                            visit_next,
                        )?;

                        remove_b = true;
                        Node::Edge(a_node)
                    } else if common.is_empty() {
                        // edge have no common path, we replace with a binary node

                        // if true, the a subtree is the right child and b subtree the left one
                        let diverg = a_node.path.0[0];

                        let a_suffix = &a_node.path.0[1..];
                        let b_suffix = &b_node.path.0[1..];

                        // new child node for a
                        let a_new_edge = if a_suffix.is_empty() {
                            // use child instead
                            a_node.child
                        } else {
                            let new_node = a_next_id.next_id();
                            let path = a_node.path.0[1..].to_bitvec();
                            pending_insertion.push((
                                new_node,
                                Node::Edge(EdgeNode {
                                    hash: None,
                                    height: 0,
                                    path: Path(path),
                                    child: a_node.child,
                                }),
                            ));
                            NodeHandle::InMemory(new_node)
                        };

                        // we need to copy the b subtree to the a map
                        let subtree_from_b = if b_suffix.is_empty() {
                            // use child instead
                            remove_b = true;
                            match b_node.child {
                                NodeHandle::Hash(felt) => NodeHandle::Hash(felt),
                                NodeHandle::InMemory(nodeid_b) => {
                                    let subtree_from_b = a_next_id.next_id();
                                    pending_copy.push(PendingSubtreeCopy {
                                        a_id: subtree_from_b,
                                        b_subtree: nodeid_b,
                                    });
                                    NodeHandle::InMemory(subtree_from_b)
                                }
                            }
                        } else {
                            b_node.path.0.drain(0..1);
                            let subtree_from_b = a_next_id.next_id();
                            pending_copy.push(PendingSubtreeCopy {
                                a_id: subtree_from_b,
                                b_subtree: nodeid_b,
                            });
                            NodeHandle::InMemory(subtree_from_b)
                        };

                        let (left, right) = if diverg {
                            (subtree_from_b, a_new_edge)
                        } else {
                            (a_new_edge, subtree_from_b)
                        };

                        Node::Binary(BinaryNode {
                            hash: None,
                            height: 0,
                            left,
                            right,
                        })
                    } else {
                        // edges have common prefix, split them and revisit them later

                        let a_suffix = &a_node.path.0[common.len()..];
                        let b_suffix = &b_node.path.0[common.len()..];

                        // new node for a
                        let a_new_edge = if a_suffix.is_empty() {
                            // use child instead
                            a_node.child
                        } else {
                            let new_node = a_next_id.next_id();
                            pending_insertion.push((
                                new_node,
                                Node::Edge(EdgeNode {
                                    hash: None,
                                    height: 0,
                                    path: Path(a_suffix.to_bitvec()),
                                    child: a_node.child,
                                }),
                            ));
                            NodeHandle::InMemory(new_node)
                        };

                        a_node.path.0 = common.to_bitvec();
                        a_node.child = a_new_edge;

                        b_node.path.0 = b_suffix.to_bitvec();
                        match a_new_edge {
                            NodeHandle::Hash(_) => {}
                            NodeHandle::InMemory(a_new_edge) => {
                                visit_next.push((a_new_edge, nodeid_b));
                            }
                        }

                        Node::Edge(a_node)
                    }
                }
            };
            *a = new_a;

            // mut ref to node a and b are dropped we can now edit a_map and b_map
            log::trace!("Current step: node_a={a:?}, node_b={b:?}");
            log::trace!("remove_b={remove_b:?} pending_insertion={pending_insertion:?}, pending_copy={pending_copy:?}, visit_next={visit_next:?}");

            if remove_b {
                b.remove();
            }

            // this clears the two pending lists
            a_map.0.extend(pending_insertion.drain(..));
            for PendingSubtreeCopy { a_id, b_subtree } in pending_copy.drain(..) {
                copy_subtree::<DB>(a_map, a_next_id, b_map, a_id, b_subtree)?;
            }

            Ok(())
        }

        match (&mut self.root_node, other.root_node) {
            (a @ _, b @ (None | Some(RootHandle::Empty))) => *a = b, // empty or unloaded tree
            (None | Some(RootHandle::Empty), Some(RootHandle::Loaded(nodeid_b))) => {
                // copy whole of b to a
                let new_a_node = self.latest_node_id.next_id();
                copy_subtree::<DB>(
                    &mut self.storage_nodes,
                    &mut self.latest_node_id,
                    &mut other.storage_nodes,
                    new_a_node,
                    nodeid_b,
                )?;
                self.root_node = Some(RootHandle::Loaded(new_a_node));
            }
            (Some(RootHandle::Loaded(nodeid_a)), Some(RootHandle::Loaded(nodeid_b))) => {
                // merge a and b
                let mut visit_next = vec![(*nodeid_a, nodeid_b)];
                while let Some((nodeid_a, nodeid_b)) = visit_next.pop() {
                    merge_nodeid::<DB>(
                        nodeid_a,
                        &mut self.storage_nodes,
                        &mut self.latest_node_id,
                        nodeid_b,
                        &mut other.storage_nodes,
                        &mut vec![],
                        &mut vec![],
                        &mut visit_next,
                    )?;
                }
            }
        }

        assert_eq!(other.storage_nodes.0.len(), 0);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::fmt::{self, Debug};

    use crate::databases::HashMapDb;
    use crate::id::BasicId;
    use crate::key_value_db::KeyValueDB;
    use crate::trie::merkle_node::{Node, NodeHandle, NodeId};
    use crate::trie::merkle_tree::{MerkleTree, RootHandle};
    use crate::HashMap;

    use bitvec::bitvec;
    use bitvec::order::Msb0;
    use bitvec::vec::BitVec;
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest_derive::Arbitrary;
    use smallvec::smallvec;
    use starknet_types_core::felt::Felt;
    use starknet_types_core::hash::{Pedersen, StarkHash};

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

    #[derive(Debug)]
    struct MerkleTreeMergeProblem {
        inserts_a: Vec<(BitVec<u8, Msb0>, Felt)>,
        inserts_b: Vec<(BitVec<u8, Msb0>, Felt)>,
    }

    impl Arbitrary for MerkleTreeMergeProblem {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
            let bitvec251 = <[bool; 251]>::arbitrary()
                .prop_map(|arr| arr.into_iter().collect::<BitVec<u8, Msb0>>());

            let key = <[bool; 3]>::arbitrary()
                .prop_map(|arr| arr.into_iter().collect::<BitVec<u8, Msb0>>());

            let felt = bitvec251
                .clone()
                .prop_map(|vec| Felt::from_bytes_be(vec.as_raw_slice().try_into().unwrap()));

            let inserts = vec((key, felt), 0..5);

            (inserts.clone(), inserts)
                .prop_map(|(inserts_a, inserts_b)| MerkleTreeMergeProblem {
                    inserts_a,
                    inserts_b,
                })
                .boxed()
        }
    }

    impl MerkleTreeMergeProblem {
        fn assert_tries_equal<H: StarkHash>(a: &MerkleTree<H>, b: &MerkleTree<H>) {
            fn assert_tries_equal_handle<H: StarkHash>(
                a_handle: &NodeHandle,
                a: &MerkleTree<H>,
                b_handle: &NodeHandle,
                b: &MerkleTree<H>,
            ) {
                match (a_handle, b_handle) {
                    (NodeHandle::Hash(a), NodeHandle::Hash(b)) => {
                        if a != b {
                            panic!("felt {:?}, {:?} do not match", a, b)
                        }
                    }
                    (NodeHandle::InMemory(a_id), NodeHandle::InMemory(b_id)) => {
                        assert_tries_equal_nodeid(*a_id, a, *b_id, b)
                    }
                    (a, b) => panic!("node handle {:?}, {:?} do not match", a, b),
                }
            }

            fn assert_tries_equal_nodeid<H: StarkHash>(
                a_id: NodeId,
                a: &MerkleTree<H>,
                b_id: NodeId,
                b: &MerkleTree<H>,
            ) {
                let a_node = a.storage_nodes.0.get(&a_id).unwrap();
                let b_node = b.storage_nodes.0.get(&b_id).unwrap();

                match (a_node, b_node) {
                    (Node::Binary(a_node), Node::Binary(b_node)) => {
                        // if a_node.height != b_node.height {
                        //     panic!("height {:?}, {:?} do not match", a_node, b_node)
                        // }
                        assert_tries_equal_handle(&a_node.left, a, &b_node.left, b);
                        assert_tries_equal_handle(&a_node.right, a, &b_node.right, b);
                    }
                    (Node::Edge(a_node), Node::Edge(b_node)) => {
                        // if a_node.height != b_node.height {
                        //     panic!("height {:?}, {:?} do not match", a_node, b_node)
                        // }
                        if a_node.path != b_node.path {
                            panic!("height {:?}, {:?} do not match", a_node, b_node)
                        }
                        assert_tries_equal_handle(&a_node.child, a, &b_node.child, b);
                    }
                    (a, b) => panic!("node {:?}, {:?} do not match", a, b),
                }
            }

            match (&a.root_node, &b.root_node) {
                (None, None) => {}
                (Some(a_handle), Some(b_handle)) => match (a_handle, b_handle) {
                    (RootHandle::Empty, RootHandle::Empty) => {}
                    (RootHandle::Loaded(a_id), RootHandle::Loaded(b_id)) => {
                        assert_tries_equal_nodeid(*a_id, a, *b_id, b);
                    }
                    (a, b) => panic!("root handle {:?}, {:?} do not match", a, b),
                },
                (a, b) => panic!("root node {:?}, {:?} do not match", a, b),
            }
        }

        fn check(&self) {
            let hashmap_db = KeyValueDB::<_, BasicId>::new(
                HashMapDb::<BasicId>::default(),
                Default::default(),
                None,
            );

            let mut tree_a = MerkleTree::<Pedersen>::new(smallvec![]);
            for (k, v) in &self.inserts_a {
                tree_a.set(&hashmap_db, &k, *v).unwrap();
            }

            let mut tree_b = MerkleTree::<Pedersen>::new(smallvec![]);
            for (k, v) in &self.inserts_b {
                tree_b.set(&hashmap_db, &k, *v).unwrap();
            }

            let mut tree_total = MerkleTree::<Pedersen>::new(smallvec![]);
            for (k, v) in self.inserts_a.iter().chain(&self.inserts_b) {
                tree_total.set(&hashmap_db, &k, *v).unwrap();
            }

            log::trace!("TREE A");
            tree_a.display();
            log::trace!("TREE B");
            tree_b.display();
            log::trace!("TARGET TREE");
            tree_total.display();

            tree_a.merge::<HashMapDb<BasicId>>(tree_b).unwrap();

            Self::assert_tries_equal(&tree_a, &tree_total);
        }
    }

    proptest::proptest! {
        #[test]
        fn merge_trees(pb in any::<MerkleTreeMergeProblem>()) {
            let _ = env_logger::builder().is_test(true).try_init();
            log::set_max_level(log::LevelFilter::Trace);
            pb.check();
        }
        #[test]
        fn inserts(pb in any::<MerkleTreeInsertProblem>()) {
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
        let pb = MerkleTreeInsertProblem(
            vec![
                Insert(
                    Key(bitvec![u8, Msb0; 1,0,0,1,1]),
                    Value(Felt::from_hex("0x20").unwrap()),
                ),
                Remove(
                    Key(bitvec![u8, Msb0; 1,0,0,1,1]),
                ),
                Remove(
                    Key(bitvec![u8, Msb0; 0,0,0,0,0]),
                ),
                Insert(
                    Key(bitvec![u8, Msb0; 0,0,0,0,0]),
                    Value(Felt::from_hex("0x20").unwrap()),
                ),
                Commit,
                Remove(
                    Key(bitvec![u8, Msb0; 0,0,0,0,0]),
                ),
            ],
        );

        pb.check();
    }

    #[test]
    fn test_merkle_pb_2() {
        use Step::*;
        let _ = env_logger::builder().is_test(true).try_init();
        log::set_max_level(log::LevelFilter::Trace);
        let pb = MerkleTreeInsertProblem(
            vec![
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
                Remove(
                    Key(bitvec![u8, Msb0; 0,0,0,0,0]),
                ),
                Commit,
            ],
        );

        pb.check();
    }

    #[test]
    fn test_merkle_pb_3() {
        use Step::*;
        let _ = env_logger::builder().is_test(true).try_init();
        log::set_max_level(log::LevelFilter::Trace);
        let pb = MerkleTreeInsertProblem(
            vec![
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
                Remove(
                    Key(bitvec![u8, Msb0; 1,0,0,0,0]),
                ),
                Remove(
                    Key(bitvec![u8, Msb0; 1,0,0,0,0]),
                ),
                Commit,
            ],
        );

        pb.check();
    }

    #[test]
    fn test_merkle_pb_4() {
        use Step::*;
        let _ = env_logger::builder().is_test(true).try_init();
        log::set_max_level(log::LevelFilter::Trace);
        let pb = MerkleTreeInsertProblem(
            vec![
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
            ],
        );

        pb.check();
    }

    #[test]
    fn test_merkle_pb_5() {
        use Step::*;
        let _ = env_logger::builder().is_test(true).try_init();
        log::set_max_level(log::LevelFilter::Trace);
        let pb = MerkleTreeInsertProblem(
            vec![
                Insert(
                    Key(bitvec![u8, Msb0; 0,0,0,0,1]),
                    Value(Felt::from_hex("0x20").unwrap()),
                ),
                Insert(
                    Key(bitvec![u8, Msb0; 0,0,1,0,0]),
                    Value(Felt::from_hex("0x20").unwrap()),
                ),
            ],
        );

        pb.check();
    }

    #[test]
    fn test_merkle_pb_6() {
        use Step::*;
        let _ = env_logger::builder().is_test(true).try_init();
        log::set_max_level(log::LevelFilter::Trace);
        let pb = MerkleTreeInsertProblem(
            vec![
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
                Remove(
                    Key(bitvec![u8, Msb0; 1,0,0,0,0]),
                ),
                Remove(
                    Key(bitvec![u8, Msb0; 1,0,0,0,0]),
                ),
            ],
        );

        pb.check();
    }
}
