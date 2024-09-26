use super::{
    merkle_node::{Direction, Node, NodeHandle, NodeId},
    path::Path,
    tree::MerkleTree,
    trie_db::TrieKeyType,
    TrieKey,
};
use crate::{
    id::Id, key_value_db::KeyValueDB, trie::tree::NodesMapping, BitSlice, BonsaiDatabase, BonsaiStorageError, ByteVec
};
use core::fmt;
use starknet_types_core::hash::StarkHash;

pub struct MerkleTreeIterator<'a, H: StarkHash, DB: BonsaiDatabase, ID: Id> {
    pub(crate) tree: &'a mut MerkleTree<H>,
    pub(crate) db: &'a KeyValueDB<DB, ID>,
    pub(crate) current_path: Path,
    pub(crate) cur_path_nodes_heights: Vec<(NodeId, usize)>,
    pub(crate) current_value: Option<NodeHandle>,
}

impl<'a, H: StarkHash, DB: BonsaiDatabase, ID: Id> fmt::Debug
    for MerkleTreeIterator<'a, H, DB, ID>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MerkleTreeIterator")
            .field("cur_path", &self.current_path)
            .field("cur_path_nodes_heights", &self.cur_path_nodes_heights)
            .field("current_value", &self.current_value)
            .finish()
    }
}

impl<'a, H: StarkHash, DB: BonsaiDatabase, ID: Id> MerkleTreeIterator<'a, H, DB, ID> {
    pub fn new(tree: &'a mut MerkleTree<H>, db: &'a KeyValueDB<DB, ID>) -> Self {
        Self {
            tree,
            db,
            current_path: Default::default(),
            cur_path_nodes_heights: Vec::with_capacity(251),
            current_value: None,
        }
    }

    #[cfg(test)]
    pub fn cur_nodes(&self) -> Vec<NodeId> {
        self.cur_path_nodes_heights
            .iter()
            .map(|n| n.0)
            .collect::<Vec<_>>()
    }

    /// Returns `true` if it has found a match.
    pub fn seek_to(&mut self, key: &BitSlice) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        self.current_value = None;

        // First, truncate the curent path and nodes list to match the new key.

        let shared_prefix_len = self
            .current_path
            .iter()
            .zip(key)
            .take_while(|(a, b)| *a == *b)
            .count();

        let nodes_new_len = if shared_prefix_len == 0 {
            0
        } else {
            // partition point is a binary search under the hood
            // TODO(perf): measure whether binary search is actually better than reverse iteration - the happy path may be that
            //  only the last few bits are different.
            let nodes_new_len = self
                .cur_path_nodes_heights
                .partition_point(|(_node, height)| *height < shared_prefix_len);
            nodes_new_len + 1
        };
        self.cur_path_nodes_heights.truncate(nodes_new_len);

        log::trace!(
            "Truncate node id cache shared_prefix_len={:?}, nodes_new_len={:?}, cur_path_nodes_heights={:?}",
            shared_prefix_len, nodes_new_len,
            self.cur_path_nodes_heights
        );

        // At first, there are no node in `cur_path_nodes` - which means `next_to_visit` will be `None`. This
        // signals that the next node to visit is the root node.
        let mut next_to_visit = if let Some((node_id, height)) = self.cur_path_nodes_heights.last()
        {
            self.current_path.truncate(*height);
            let node = self.tree.node_storage.nodes.0.get_mut(node_id).ok_or(
                // Dangling node id in memory
                BonsaiStorageError::Trie("Could not get node from temp storage".to_string()),
            )?;
            match node {
                Node::Binary(binary_node) => {
                    log::trace!(
                        "Continue from binary node current_path={:?} key={:b}",
                        self.current_path,
                        key,
                    );
                    let next_direction = Direction::from(key[self.current_path.len() - 1]);
                    *self.current_path.last_mut().expect(
                        "current path can't be empty if cur_path_nodes_heights is not empty",
                    ) = bool::from(next_direction);
                    Some(binary_node.get_child_mut(next_direction))
                }
                Node::Edge(edge_node)
                    if edge_node
                        .path_matches_(key, height.saturating_sub(edge_node.path.len())) =>
                {
                    Some(&mut edge_node.child)
                }

                // We are in a case where the edge node doesn't match the path we want to preload so we return nothing.
                Node::Edge(_) => return Ok(()),
            }
        } else {
            self.current_path.clear();
            // Start from tree root.
            None
        };

        log::trace!(
            "Starting traversal with path {:?}, next={:?}",
            self.current_path,
            next_to_visit
        );

        // Tree traversal :)

        loop {
            log::trace!("Loop start cur={:b} key={:b}", self.current_path.0, key);
            if self.current_path.len() > key.len() {
                // We overshot. Keep the last value.
                return Ok(()); // end of traversal
            }
            if self.current_path.len() == key.len() {
                // Exact match, no overshoot: return this new value.
                self.current_value = next_to_visit.as_ref().map(|c| **c);
                return Ok(()); // end of traversal
            }
            self.current_value = next_to_visit.as_ref().map(|c| **c);

            // get node from cache or database
            let (node_id, node) = match next_to_visit {
                // tree root
                None => {
                    match self.tree.node_storage.nodes.get_root_node(
                        &mut self.tree.node_storage.root_node,
                        &mut self.tree.node_storage.latest_node_id,
                        &self.tree.death_row,
                        &self.tree.identifier,
                        &self.db,
                    )? {
                        Some((node_id, node)) => (node_id, node),
                        None => return Ok(()), // empty tree, not found
                    }
                }
                // not tree root
                Some(next_to_visit) => match *next_to_visit {
                    NodeHandle::Hash(_) => {
                        // load from db

                        // TODO(perf): useless allocs everywhere here...
                        let path: ByteVec = self.current_path.clone().into();
                        log::trace!("Visiting db node {:?}", self.current_path);
                        let key = TrieKey::new(&self.tree.identifier, TrieKeyType::Trie, &path);
                        let Some((node_id, node)) = NodesMapping::load_db_node_get_id(
                            &mut self.tree.node_storage.latest_node_id,
                            &self.tree.death_row,
                            &self.db,
                            &key,
                        )?
                        else {
                            // Dangling node id in db
                            return Err(BonsaiStorageError::Trie(
                                "Could not get node from db".to_string(),
                            ));
                        };
                        *next_to_visit = NodeHandle::InMemory(node_id);
                        let node = self.tree.node_storage.nodes.load_db_node_to_id::<DB>(node_id, node)?;
                        (node_id, node)
                    }
                    NodeHandle::InMemory(node_id) => {
                        log::trace!("Visiting inmemory node {:?}", self.current_path);
                        let node = self.tree.node_storage.nodes.0.get_mut(&node_id).ok_or(
                            // Dangling node id in memory
                            BonsaiStorageError::Trie(
                                "Could not get node from temp storage".to_string(),
                            ),
                        )?;

                        (node_id, node)
                    }
                },
            };

            // visit the child
            match node {
                Node::Binary(binary_node) => {
                    let next_direction = Direction::from(key[self.current_path.len() as usize]);
                    self.current_path.push(bool::from(next_direction));
                    next_to_visit = Some(binary_node.get_child_mut(next_direction));
                }

                Node::Edge(edge_node) if edge_node.path_matches_(key, self.current_path.len()) => {
                    self.current_path.extend_from_bitslice(&edge_node.path);
                    next_to_visit = Some(&mut edge_node.child);
                }

                // We are in a case where the edge node doesn't match the path we want to preload so we return nothing.
                Node::Edge(edge_node) => {
                    self.current_path.extend_from_bitslice(&edge_node.path);
                    self.cur_path_nodes_heights
                        .push((node_id, self.current_path.len()));
                    return Ok(());
                }
            }

            self.cur_path_nodes_heights
                .push((node_id, self.current_path.len()));

            log::trace!(
                "Got nodeid={:?} height={}, cur path={:?}, next to visit={:?}",
                node_id,
                self.current_path.len(),
                self.current_path,
                next_to_visit
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        databases::{create_rocks_db, RocksDB, RocksDBConfig},
        id::BasicId,
        trie::{
            iterator::MerkleTreeIterator,
            merkle_node::{NodeHandle, NodeId},
        },
        BonsaiStorage, BonsaiStorageConfig,
    };
    use bitvec::{bits, order::Msb0};
    use starknet_types_core::{felt::Felt, hash::Pedersen};

    #[test]
    fn test_iterator_seek_to() {
        let _ = env_logger::builder().is_test(true).try_init();
        log::set_max_level(log::LevelFilter::Trace);
        let tempdir = tempfile::tempdir().unwrap();
        let db = create_rocks_db(tempdir.path()).unwrap();
        let mut bonsai_storage: BonsaiStorage<BasicId, _, Pedersen> = BonsaiStorage::new(
            RocksDB::<BasicId>::new(&db, RocksDBConfig::default()),
            BonsaiStorageConfig::default(),
        )
        .unwrap();

        #[allow(non_snake_case)]
        let [ONE, TWO, THREE, FOUR] = [
            Felt::ONE,
            Felt::TWO,
            Felt::THREE,
            Felt::from_hex_unchecked("0x4"),
        ];

        bonsai_storage
            .insert(&[], &bits![u8, Msb0; 0,0,0,1,0,0,0,0], &ONE)
            .unwrap();
        bonsai_storage
            .insert(&[], &bits![u8, Msb0; 0,0,0,1,0,0,0,1], &TWO)
            .unwrap();
        bonsai_storage
            .insert(&[], &bits![u8, Msb0; 0,0,0,1,0,0,1,0], &THREE)
            .unwrap();
        bonsai_storage
            .insert(&[], &bits![u8, Msb0; 0,1,0,0,0,0,0,0], &FOUR)
            .unwrap();

        bonsai_storage.dump();

        // Trie

        let tree = bonsai_storage
            .tries
            .trees
            .get_mut(&smallvec::smallvec![])
            .unwrap();
        let mut iter = MerkleTreeIterator::new(tree, &bonsai_storage.tries.db);

        // SEEK TO LEAF

        // from scratch, should find the leaf
        iter.seek_to(&bits![u8, Msb0; 0,0,0,1,0,0,0,0]).unwrap();
        assert_eq!(iter.current_value, Some(NodeHandle::Hash(ONE)));
        assert_eq!(
            iter.cur_nodes(),
            vec![NodeId(1), NodeId(7), NodeId(6), NodeId(4), NodeId(2)]
        );
        println!("{iter:?}");
        // from a closeby leaf, should backtrack and find the next one
        iter.seek_to(&bits![u8, Msb0; 0,0,0,1,0,0,0,1]).unwrap();
        assert_eq!(iter.current_value, Some(NodeHandle::Hash(TWO)));
        assert_eq!(
            iter.cur_nodes(),
            vec![NodeId(1), NodeId(7), NodeId(6), NodeId(4), NodeId(2)]
        );
        println!("{iter:?}");
        // backtrack farther, should find the leaf
        iter.seek_to(&bits![u8, Msb0; 0,0,0,1,0,0,1,0]).unwrap();
        assert_eq!(iter.current_value, Some(NodeHandle::Hash(THREE)));
        assert_eq!(
            iter.cur_nodes(),
            vec![NodeId(1), NodeId(7), NodeId(6), NodeId(4), NodeId(3)]
        );
        println!("{iter:?}");
        // backtrack farther, should find the leaf
        iter.seek_to(&bits![u8, Msb0; 0,1,0,0,0,0,0,0]).unwrap();
        assert_eq!(iter.current_value, Some(NodeHandle::Hash(FOUR)));
        assert_eq!(iter.cur_nodes(), vec![NodeId(1), NodeId(7), NodeId(5)]);
        println!("{iter:?}");

        // similar case
        iter.seek_to(&bits![u8, Msb0; 0,0,0,1,0,0,0,1]).unwrap();
        assert_eq!(iter.current_value, Some(NodeHandle::Hash(TWO)));
        assert_eq!(
            iter.cur_nodes(),
            vec![NodeId(1), NodeId(7), NodeId(6), NodeId(4), NodeId(2)]
        );
        println!("{iter:?}");

        // SEEK MIDWAY INTO THE TREE

        // jump midway into an edge
        iter.seek_to(&bits![u8, Msb0; 0,1,0,0,0]).unwrap();
        // The current value should reflect the edge
        assert_eq!(iter.current_value, Some(NodeHandle::InMemory(NodeId(5))));
        // The current path should reflect the tip of the edge
        assert_eq!(iter.current_path.0, bits![u8, Msb0; 0,1,0,0,0,0,0,0]);
        assert_eq!(iter.cur_nodes(), vec![NodeId(1), NodeId(7), NodeId(5)]);
        println!("{iter:?}");

        // jump midway into an edge, but its child is not a leaf
        iter.seek_to(&bits![u8, Msb0; 0,0,0]).unwrap();
        // The current value should reflect the edge
        assert_eq!(iter.current_value, Some(NodeHandle::InMemory(NodeId(6))));
        // The current path should reflect the edge
        assert_eq!(iter.current_path.0, bits![u8, Msb0; 0,0,0,1,0,0]);
        assert_eq!(iter.cur_nodes(), vec![NodeId(1), NodeId(7), NodeId(6)]);
        println!("{iter:?}");

        // jump to a binary node
        iter.seek_to(&bits![u8, Msb0; 0,0,0,1,0,0,0]).unwrap();
        // The current value should reflect the binary node
        assert_eq!(iter.current_value, Some(NodeHandle::InMemory(NodeId(2))));
        assert_eq!(iter.current_path.0, bits![u8, Msb0; 0,0,0,1,0,0,0]);
        assert_eq!(
            iter.cur_nodes(),
            vec![NodeId(1), NodeId(7), NodeId(6), NodeId(4)]
        );
        println!("{iter:?}");

        // jump to the end of an edge
        iter.seek_to(&bits![u8, Msb0; 0,0,0,1,0,0]).unwrap();
        // The current value should reflect the binary node
        assert_eq!(iter.current_value, Some(NodeHandle::InMemory(NodeId(4))));
        // The current path should reflect the tip of the edge
        assert_eq!(iter.current_path.0, bits![u8, Msb0; 0,0,0,1,0,0]);
        assert_eq!(iter.cur_nodes(), vec![NodeId(1), NodeId(7), NodeId(6)]);
        println!("{iter:?}");

        // jump to top
        iter.seek_to(&bits![u8, Msb0; ]).unwrap();
        assert_eq!(iter.current_value, None);
        assert_eq!(iter.current_path.0, bits![u8, Msb0; ]);
        assert_eq!(iter.cur_nodes(), vec![]);
        println!("{iter:?}");

        // jump to first node
        iter.seek_to(&bits![u8, Msb0; 0]).unwrap();
        assert_eq!(iter.current_value, Some(NodeHandle::InMemory(NodeId(7))));
        assert_eq!(iter.current_path.0, bits![u8, Msb0; 0]);
        assert_eq!(iter.cur_nodes(), vec![NodeId(1)]);
        println!("{iter:?}");

        // jump to non existent node, returning same edge
        iter.seek_to(&bits![u8, Msb0; 0,1,0,1,0,0,0]).unwrap();
        assert_eq!(iter.current_value, Some(NodeHandle::InMemory(NodeId(5))));
        assert_eq!(iter.current_path.0, bits![u8, Msb0; 0,1,0,0,0,0,0,0]);
        assert_eq!(iter.cur_nodes(), vec![NodeId(1), NodeId(7), NodeId(5)]);
        println!("{iter:?}");

        // jump to non existent node, deviating from edge, should not go into the children
        iter.seek_to(&bits![u8, Msb0; 1,0,0,1,0,0,0]).unwrap();
        assert_eq!(iter.current_value, None);
        assert_eq!(iter.current_path.0, bits![u8, Msb0; 0]);
        assert_eq!(iter.cur_nodes(), vec![NodeId(1)]);
        println!("{iter:?}");

        // jump to non existent node, deviating from first node
        iter.seek_to(&bits![u8, Msb0; 1]).unwrap();
        assert_eq!(iter.current_value, None);
        assert_eq!(iter.current_path.0, bits![u8, Msb0; 0]);
        assert_eq!(iter.cur_nodes(), vec![NodeId(1)]);
        println!("{iter:?}");
    }
}
