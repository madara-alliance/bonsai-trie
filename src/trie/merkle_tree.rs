use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use bitvec::{
    prelude::{BitSlice, BitVec, Msb0},
    view::BitView,
};
use core::iter::once;
use core::marker::PhantomData;
use core::mem;
use derive_more::Constructor;
#[cfg(not(feature = "std"))]
use hashbrown::HashMap;
use parity_scale_codec::{Decode, Encode, Error, Input, Output};
use starknet_types_core::{felt::Felt, hash::StarkHash};
#[cfg(feature = "std")]
use std::collections::HashMap;

use crate::{error::BonsaiStorageError, id::Id, BonsaiDatabase, KeyValueDB};

use super::{
    merkle_node::{BinaryNode, Direction, EdgeNode, Node, NodeHandle, NodeId},
    TrieKeyType,
};

#[cfg(test)]
use log::trace;

/// Wrapper type for a [HashMap<NodeId, Node>] object. (It's not really a wrapper it's a
/// copy of the type but we implement the necessary traits.)
#[derive(Clone, Debug, PartialEq, Eq, Default, Constructor)]
pub struct NodesMapping(pub HashMap<NodeId, Node>);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path(pub BitVec<u8, Msb0>);

impl Encode for Path {
    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        // Inspired from scale_bits crate but don't use it to avoid copy and u32 length encoding
        let iter = self.0.iter();
        let len = iter.len();
        // SAFETY: len is <= 251
        dest.push_byte(len as u8);
        let mut next_store: u8 = 0;
        let mut pos_in_next_store: u8 = 7;
        for b in iter {
            let bit = match *b {
                true => 1,
                false => 0,
            };
            next_store |= bit << pos_in_next_store;

            if pos_in_next_store == 0 {
                pos_in_next_store = 8;
                dest.push_byte(next_store);
                next_store = 0;
            }
            pos_in_next_store -= 1;
        }

        if pos_in_next_store < 7 {
            dest.push_byte(next_store);
        }
    }
}

impl Decode for Path {
    fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
        // Inspired from scale_bits crate but don't use it to avoid copy and u32 length encoding
        // SAFETY: len is <= 251
        let len: u8 = input.read_byte()?;
        let mut remaining_bits = len as usize;
        let mut current_byte = None;
        let mut bit = 7;
        let mut bits = BitVec::<u8, Msb0>::new();
        // No bits left to decode; we're done.
        while remaining_bits != 0 {
            // Get the next store entry to pull from:
            let store = match current_byte {
                Some(store) => store,
                None => {
                    let store = match input.read_byte() {
                        Ok(s) => s,
                        Err(e) => return Err(e),
                    };
                    current_byte = Some(store);
                    store
                }
            };

            // Extract a bit:
            let res = match (store >> bit) & 1 {
                0 => false,
                1 => true,
                _ => unreachable!("Can only be 0 or 1 owing to &1"),
            };
            bits.push(res);

            // Update records for next bit:
            remaining_bits -= 1;
            if bit == 0 {
                current_byte = None;
                bit = 8;
            }
            bit -= 1;
        }
        Ok(Self(bits))
    }
}

#[test]
fn test_shared_path_encode_decode() {
    let path = Path(BitVec::<u8, Msb0>::from_slice(&[0b10101010, 0b10101010]));
    let mut encoded = Vec::new();
    path.encode_to(&mut encoded);

    let decoded = Path::decode(&mut &encoded[..]).unwrap();
    assert_eq!(path, decoded);
}

/// A Starknet binary Merkle-Patricia tree with a specific root entry-point and storage.
///
/// This is used to update, mutate and access global Starknet state as well as individual contract
/// states.
///
/// For more information on how this functions internally, see [here](super::merkle_node).
pub struct MerkleTree<H: StarkHash, DB: BonsaiDatabase, ID: Id> {
    root_handle: NodeHandle,
    root_hash: Felt,
    storage_nodes: NodesMapping,
    db: KeyValueDB<DB, ID>,
    latest_node_id: NodeId,
    death_row: Vec<TrieKeyType>,
    cache_leaf_modified: HashMap<Vec<u8>, InsertOrRemove<Felt>>,
    _hasher: PhantomData<H>,
}

#[derive(Debug, PartialEq)]
enum InsertOrRemove<T> {
    Insert(T),
    Remove,
}

impl<H: StarkHash, DB: BonsaiDatabase, ID: Id> MerkleTree<H, DB, ID> {
    /// Less visible initialization for `MerkleTree<T>` as the main entry points should be
    /// [`MerkleTree::<RcNodeStorage>::load`] for persistent trees and [`MerkleTree::empty`] for
    /// transient ones.
    pub fn new(mut db: KeyValueDB<DB, ID>) -> Result<Self, BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
    {
        let nodes_mapping: HashMap<NodeId, Node> = HashMap::new();
        let root_node = db.get(&TrieKeyType::Trie(vec![]))?;
        let node = if let Some(root_node) = root_node {
            Node::decode(&mut root_node.as_slice()).map_err(|err| {
                BonsaiStorageError::Trie(format!("Couldn't decode root node: {}", err))
            })?
        } else {
            db.insert(
                &TrieKeyType::Trie(vec![]),
                &Node::Unresolved(Felt::ZERO).encode(),
                None,
            )?;
            Node::Unresolved(Felt::ZERO)
        };
        let root = node.hash().ok_or(BonsaiStorageError::Trie(
            "Root doesn't exist in the storage".to_string(),
        ))?;
        Ok(Self {
            root_handle: NodeHandle::Hash(root),
            root_hash: root,
            storage_nodes: NodesMapping(nodes_mapping),
            db,
            latest_node_id: NodeId(0),
            death_row: Vec::new(),
            cache_leaf_modified: HashMap::new(),
            _hasher: PhantomData,
        })
    }

    pub fn root_hash(&self) -> Felt {
        self.root_hash
    }

    pub fn reset_root_from_db(&mut self) -> Result<(), BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
    {
        let node = self
            .get_tree_branch_in_db_from_path(&BitVec::<u8, Msb0>::new())?
            .ok_or(BonsaiStorageError::Trie(
                "root node doesn't exist in the storage".to_string(),
            ))?;
        let node_hash = node.hash().ok_or(BonsaiStorageError::Trie(
            "Root doesn't exist in the storage".to_string(),
        ))?;
        self.latest_node_id.reset();
        self.storage_nodes.0.clear();
        self.cache_leaf_modified.clear();
        self.root_handle = NodeHandle::Hash(node_hash);
        self.root_hash = node_hash;
        Ok(())
    }

    /// Persists all changes to storage and returns the new root hash.
    pub fn commit(&mut self) -> Result<Felt, BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
    {
        let mut batch = self.db.create_batch();
        for node_key in mem::take(&mut self.death_row) {
            self.db.remove(&node_key, Some(&mut batch))?;
        }
        let root_hash = self.commit_subtree(self.root_handle, BitVec::new(), &mut batch)?;
        for (key, value) in mem::take(&mut self.cache_leaf_modified) {
            match value {
                InsertOrRemove::Insert(value) => {
                    self.db
                        .insert(&TrieKeyType::Flat(key), &value.encode(), Some(&mut batch))?;
                }
                InsertOrRemove::Remove => {
                    self.db.remove(&TrieKeyType::Flat(key), Some(&mut batch))?;
                }
            }
        }
        self.db.write_batch(batch)?;
        self.latest_node_id.reset();
        self.root_hash = root_hash;
        self.root_handle = NodeHandle::Hash(root_hash);
        Ok(root_hash)
    }

    /// Persists any changes in this subtree to storage.
    ///
    /// This necessitates recursively calculating the hash of, and
    /// in turn persisting, any changed child nodes. This is necessary
    /// as the parent node's hash relies on its children hashes.
    ///
    /// In effect, the entire tree gets persisted.
    ///
    /// # Arguments
    ///
    /// * `node` - The top node from the subtree to commit.
    fn commit_subtree(
        &mut self,
        node_handle: NodeHandle,
        path: BitVec<u8, Msb0>,
        batch: &mut DB::Batch,
    ) -> Result<Felt, BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
    {
        use Node::*;
        let node_id = match node_handle {
            NodeHandle::Hash(hash) => return Ok(hash),
            NodeHandle::InMemory(root_id) => root_id,
        };

        match self
            .storage_nodes
            .0
            .remove(&node_id)
            .ok_or(BonsaiStorageError::Trie(
                "Couldn't fetch node in the temporary storage".to_string(),
            ))? {
            Unresolved(hash) => {
                if path.is_empty() {
                    self.db.insert(
                        &TrieKeyType::Trie(vec![]),
                        &Node::Unresolved(hash).encode(),
                        Some(batch),
                    )?;
                    Ok(hash)
                } else {
                    Ok(hash)
                }
            }
            Binary(mut binary) => {
                let mut left_path = path.clone();
                left_path.push(false);
                let left_hash = self.commit_subtree(binary.left, left_path, batch)?;
                let mut right_path = path.clone();
                right_path.push(true);
                let right_hash = self.commit_subtree(binary.right, right_path, batch)?;
                let hash = H::hash(&left_hash, &right_hash);
                binary.hash = Some(hash);
                binary.left = NodeHandle::Hash(left_hash);
                binary.right = NodeHandle::Hash(right_hash);
                let key_bytes = [&[path.len() as u8], path.as_raw_slice()].concat();
                self.db.insert(
                    &TrieKeyType::Trie(key_bytes),
                    &Node::Binary(binary).encode(),
                    Some(batch),
                )?;
                Ok(hash)
            }

            Edge(mut edge) => {
                let mut child_path = path.clone();
                child_path.extend(&edge.path.0);
                let child_hash = self.commit_subtree(edge.child, child_path, batch)?;
                let mut bytes = [0u8; 32];
                bytes.view_bits_mut::<Msb0>()[256 - edge.path.0.len()..]
                    .copy_from_bitslice(&edge.path.0);

                let felt_path = Felt::from_bytes_be(&bytes);
                let mut length = [0; 32];
                // Safe as len() is guaranteed to be <= 251
                length[31] = edge.path.0.len() as u8;

                let length = Felt::from_bytes_be(&length);
                let hash = H::hash(&child_hash, &felt_path) + length;
                edge.hash = Some(hash);
                edge.child = NodeHandle::Hash(child_hash);
                let key_bytes = if path.is_empty() {
                    vec![]
                } else {
                    [&[path.len() as u8], path.as_raw_slice()].concat()
                };
                self.db.insert(
                    &TrieKeyType::Trie(key_bytes),
                    &Node::Edge(edge).encode(),
                    Some(batch),
                )?;
                Ok(hash)
            }
        }
    }

    /// Sets the value of a key. To delete a key, set the value to [Felt::ZERO].
    ///
    /// # Arguments
    ///
    /// * `key` - The key to set.
    /// * `value` - The value to set.
    pub fn set(&mut self, key: &BitSlice<u8, Msb0>, value: Felt) -> Result<(), BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
    {
        if value == Felt::ZERO {
            return self.delete_leaf(key);
        }
        let path = self.preload_nodes(key)?;
        // There are three possibilities.
        //
        // 1. The leaf exists, in which case we simply change its value.
        //
        // 2. The tree is empty, we insert the new leaf and the root becomes an edge node connecting to it.
        //
        // 3. The leaf does not exist, and the tree is not empty. The final node in the traversal will be an
        //    edge node who's path diverges from our new leaf node's.
        //
        //    This edge must be split into a new subtree containing both the existing edge's child and the
        //    new leaf. This requires an edge followed by a binary node and then further edges to both the
        //    current child and the new leaf. Any of these new edges may also end with an empty path in
        //    which case they should be elided. It depends on the common path length of the current edge
        //    and the new leaf i.e. the split may be at the first bit (in which case there is no leading
        //    edge), or the split may be in the middle (requires both leading and post edges), or the
        //    split may be the final bit (no post edge).
        use Node::*;
        match path.last() {
            Some(node_id) => {
                let mut nodes_to_add = Vec::with_capacity(4);
                self.storage_nodes.0.entry(*node_id).and_modify(|node| {
                    match node {
                        Edge(edge) => {
                            let common = edge.common_path(key);
                            // Height of the binary node
                            let branch_height = edge.height as usize + common.len();
                            if branch_height == key.len() {
                                edge.child = NodeHandle::Hash(value);
                                // The leaf already exists, we simply change its value.
                                let key_bytes =
                                    [&[key.len() as u8], key.to_bitvec().as_raw_slice()].concat();
                                self.cache_leaf_modified
                                    .insert(key_bytes, InsertOrRemove::Insert(value));
                                return;
                            }
                            // Height of the binary node's children
                            let child_height = branch_height + 1;

                            // Path from binary node to new leaf
                            let new_path = key[child_height..].to_bitvec();
                            // Path from binary node to existing child
                            let old_path = edge.path.0[common.len() + 1..].to_bitvec();

                            // The new leaf branch of the binary node.
                            // (this may be edge -> leaf, or just leaf depending).
                            let key_bytes =
                                [&[key.len() as u8], key.to_bitvec().as_raw_slice()].concat();
                            self.cache_leaf_modified
                                .insert(key_bytes, InsertOrRemove::Insert(value));

                            let new = if new_path.is_empty() {
                                NodeHandle::Hash(value)
                            } else {
                                let new_edge = Node::Edge(EdgeNode {
                                    hash: None,
                                    height: child_height as u64,
                                    path: Path(new_path),
                                    child: NodeHandle::Hash(value),
                                });
                                let edge_id = self.latest_node_id.next_id();
                                nodes_to_add.push((edge_id, new_edge));
                                NodeHandle::InMemory(edge_id)
                            };

                            // The existing child branch of the binary node.
                            let old = if old_path.is_empty() {
                                edge.child
                            } else {
                                let old_edge = Node::Edge(EdgeNode {
                                    hash: None,
                                    height: child_height as u64,
                                    path: Path(old_path),
                                    child: edge.child,
                                });
                                let edge_id = self.latest_node_id.next_id();
                                nodes_to_add.push((edge_id, old_edge));
                                NodeHandle::InMemory(edge_id)
                            };

                            let new_direction = Direction::from(key[branch_height]);
                            let (left, right) = match new_direction {
                                Direction::Left => (new, old),
                                Direction::Right => (old, new),
                            };

                            let branch = Node::Binary(BinaryNode {
                                hash: None,
                                height: branch_height as u64,
                                left,
                                right,
                            });

                            // We may require an edge leading to the binary node.
                            let new_node = if common.is_empty() {
                                branch
                            } else {
                                let branch_id = self.latest_node_id.next_id();
                                nodes_to_add.push((branch_id, branch));

                                Node::Edge(EdgeNode {
                                    hash: None,
                                    height: edge.height,
                                    path: Path(common.to_bitvec()),
                                    child: NodeHandle::InMemory(branch_id),
                                })
                            };
                            let path = key[..edge.height as usize].to_bitvec();
                            let key_bytes =
                                [&[path.len() as u8], path.into_vec().as_slice()].concat();
                            self.death_row.push(TrieKeyType::Trie(key_bytes));
                            *node = new_node;
                        }
                        Unresolved(_) | Binary(_) => {
                            unreachable!("The end of a traversion cannot be unresolved or binary")
                        }
                    };
                });
                for (id, node) in nodes_to_add {
                    self.storage_nodes.0.insert(id, node);
                }
                Ok(())
            }
            None => {
                // Getting no travel nodes implies that the tree is empty.
                //
                // Create a new leaf node with the value, and the root becomes
                // an edge node connecting to the leaf.
                let edge = Node::Edge(EdgeNode {
                    hash: None,
                    height: 0,
                    path: Path(key.to_bitvec()),
                    child: NodeHandle::Hash(value),
                });
                self.storage_nodes
                    .0
                    .insert(self.latest_node_id.next_id(), edge);

                self.root_handle = NodeHandle::InMemory(self.latest_node_id);

                let key_bytes = [&[key.len() as u8], key.to_bitvec().as_raw_slice()].concat();
                self.cache_leaf_modified
                    .insert(key_bytes, InsertOrRemove::Insert(value));
                Ok(())
            }
        }
    }

    pub fn db_ref(&self) -> &KeyValueDB<DB, ID> {
        &self.db
    }

    pub fn db(self) -> KeyValueDB<DB, ID> {
        self.db
    }

    pub fn db_mut(&mut self) -> &mut KeyValueDB<DB, ID> {
        &mut self.db
    }

    /// Deletes a leaf node from the tree.
    ///
    /// This is not an external facing API; the functionality is instead accessed by calling
    /// [`MerkleTree::set`] with value set to [`Felt::ZERO`].
    ///
    /// # Arguments
    ///
    /// * `key` - The key to delete.
    fn delete_leaf(&mut self, key: &BitSlice<u8, Msb0>) -> Result<(), BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
    {
        // Algorithm explanation:
        //
        // The leaf's parent node is either an edge, or a binary node.
        // If it's an edge node, then it must also be deleted. And its parent
        // must be a binary node. In either case we end up with a binary node
        // who's one child is deleted. This changes the binary to an edge node.
        //
        // Note that its possible that there is no binary node -- if the resulting tree would be empty.
        //
        // This new edge node may need to merge with the old binary node's parent node
        // and other remaining child node -- if they're also edges.
        //
        // Then we are done.

        let key_bytes = [&[key.len() as u8], key.to_bitvec().as_raw_slice()].concat();
        self.cache_leaf_modified
            .insert(key_bytes.clone(), InsertOrRemove::Remove);
        if !self.db.contains(&TrieKeyType::Flat(key_bytes))? {
            return Ok(());
        }

        let path = self.preload_nodes(key)?;

        let mut last_binary_path = key.to_bitvec();

        // Go backwards until we hit a branch node.
        let mut node_iter = path.into_iter().rev().skip_while(|node| {
            // SAFETY: Has been populate by preload_nodes just above
            let node = self.storage_nodes.0.get(node).unwrap();
            match node {
                Node::Unresolved(_) => {}
                Node::Binary(_) => {}
                Node::Edge(edge) => {
                    for _ in 0..edge.path.0.len() {
                        last_binary_path.pop();
                    }
                    let key_bytes = [
                        &[last_binary_path.len() as u8],
                        last_binary_path.as_raw_slice(),
                    ]
                    .concat();
                    if last_binary_path.is_empty() {
                        self.death_row.push(TrieKeyType::Trie(vec![]));
                    } else {
                        self.death_row.push(TrieKeyType::Trie(key_bytes));
                    }
                }
            }
            !node.is_binary()
        });
        let branch_node = node_iter.next();
        let parent_branch_node = node_iter.next();
        match branch_node {
            Some(node_id) => {
                let new_edge =
                    {
                        let node = self.storage_nodes.0.get_mut(&node_id).ok_or(
                            BonsaiStorageError::Trie("Node not found in memory".to_string()),
                        )?;
                        let (direction, height) = {
                            // SAFETY: This node must be a binary node due to the iteration condition.
                            let binary = node.as_binary().unwrap();
                            (binary.direction(key).invert(), binary.height)
                        };
                        // Create an edge node to replace the old binary node
                        // i.e. with the remaining child (note the direction invert),
                        //      and a path of just a single bit.
                        last_binary_path.push(direction.into());
                        let path = Path(once(bool::from(direction)).collect::<BitVec<_, _>>());
                        let mut edge = EdgeNode {
                            hash: None,
                            height,
                            path,
                            child: NodeHandle::InMemory(self.latest_node_id),
                        };

                        // Merge the remaining child if it's an edge.
                        self.merge_edges(&mut edge)?;

                        edge
                    };
                // Replace the old binary node with the new edge node.
                self.storage_nodes.0.insert(node_id, Node::Edge(new_edge));
            }
            None => {
                // We reached the root without a hitting binary node. The new tree
                // must therefore be empty.
                self.latest_node_id.next_id();
                self.storage_nodes
                    .0
                    .insert(self.latest_node_id, Node::Unresolved(Felt::ZERO));
                self.root_handle = NodeHandle::InMemory(self.latest_node_id);
                self.root_hash = Felt::ZERO;
                return Ok(());
            }
        };

        // Check the parent of the new edge. If it is also an edge, then they must merge.
        if let Some(node) = parent_branch_node {
            let child = if let Node::Edge(edge) =
                self.storage_nodes
                    .0
                    .get(&node)
                    .ok_or(BonsaiStorageError::Trie(
                        "Node not found in memory".to_string(),
                    ))? {
                let child_node = match edge.child {
                    NodeHandle::Hash(_) => return Ok(()),
                    NodeHandle::InMemory(child_id) => {
                        self.storage_nodes
                            .0
                            .get(&child_id)
                            .ok_or(BonsaiStorageError::Trie(
                                "Node not found in memory".to_string(),
                            ))?
                    }
                };
                match child_node {
                    Node::Edge(child_edge) => child_edge.clone(),
                    _ => {
                        return Ok(());
                    }
                }
            } else {
                return Ok(());
            };
            let edge = self
                .storage_nodes
                .0
                .get_mut(&node)
                .ok_or(BonsaiStorageError::Trie(
                    "Node not found in memory".to_string(),
                ))?;
            match edge {
                Node::Edge(edge) => {
                    edge.path.0.extend_from_bitslice(&child.path.0);
                    edge.child = child.child;
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }

    /// Returns the value stored at key, or `None` if it does not exist.
    ///
    /// # Arguments
    ///
    /// * `key` - The key of the value to get.
    ///
    /// # Returns
    ///
    /// The value of the key.
    pub fn get(&self, key: &BitSlice<u8, Msb0>) -> Result<Option<Felt>, BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
    {
        let key = &[&[key.len() as u8], key.to_bitvec().as_raw_slice()].concat();
        if let Some(value) = self.cache_leaf_modified.get(key) {
            match value {
                InsertOrRemove::Remove => return Ok(None),
                InsertOrRemove::Insert(value) => return Ok(Some(*value)),
            }
        }
        self.db
            .get(&TrieKeyType::Flat(key.to_vec()))
            .map(|r| r.map(|opt| Felt::decode(&mut opt.as_slice()).unwrap()))
    }

    pub fn contains(&self, key: &BitSlice<u8, Msb0>) -> Result<bool, BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
    {
        let key = &[&[key.len() as u8], key.to_bitvec().as_raw_slice()].concat();
        if let Some(value) = self.cache_leaf_modified.get(key) {
            match value {
                InsertOrRemove::Remove => return Ok(false),
                InsertOrRemove::Insert(_) => return Ok(true),
            }
        }
        self.db.contains(&TrieKeyType::Flat(key.to_vec()))
    }

    /// preload_nodes from the current root towards the destination [Leaf](Node::Leaf) node.
    /// Returns the list of nodes along the path.
    ///
    /// If the destination node exists, it will be the final node in the list.
    ///
    /// This means that the final node will always be either a the destination [Leaf](Node::Leaf)
    /// node, or an [Edge](Node::Edge) node who's path suffix does not match the leaf's path.
    ///
    /// The final node can __not__ be a [Binary](Node::Binary) node since it would always be
    /// possible to continue on towards the destination. Nor can it be an
    /// [Unresolved](Node::Unresolved) node since this would be resolved to check if we can
    /// travel further.
    ///
    /// # Arguments
    ///
    /// * `dst` - The node to get to.
    ///
    /// # Returns
    ///
    /// The list of nodes along the path.
    fn preload_nodes(&mut self, dst: &BitSlice<u8, Msb0>) -> Result<Vec<NodeId>, BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
    {
        let mut nodes = Vec::with_capacity(251);
        let node_id = match self.root_handle {
            NodeHandle::Hash(_) => {
                let node = self
                    .get_tree_branch_in_db_from_path(&BitVec::<u8, Msb0>::new())?
                    .ok_or(BonsaiStorageError::Trie(
                        "Couldn't fetch root node in db".to_string(),
                    ))?;
                if node.is_empty() {
                    return Ok(Vec::new());
                }
                self.latest_node_id.next_id();
                self.root_handle = NodeHandle::InMemory(self.latest_node_id);
                self.storage_nodes.0.insert(self.latest_node_id, node);
                nodes.push(self.latest_node_id);
                self.latest_node_id
            }
            NodeHandle::InMemory(root_id) => {
                nodes.push(root_id);
                root_id
            }
        };
        self.preload_nodes_subtree(dst, node_id, BitVec::<u8, Msb0>::new(), &mut nodes)?;
        Ok(nodes)
    }

    fn preload_nodes_subtree(
        &mut self,
        dst: &BitSlice<u8, Msb0>,
        root_id: NodeId,
        mut path: BitVec<u8, Msb0>,
        nodes: &mut Vec<NodeId>,
    ) -> Result<(), BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
    {
        let node = self
            .storage_nodes
            .0
            .get(&root_id)
            .ok_or(BonsaiStorageError::Trie(
                "Couldn't fetch node in the temporary storage".to_string(),
            ))?
            .clone();
        match node {
            Node::Unresolved(_hash) => Ok(()),
            Node::Binary(mut binary_node) => {
                let next_direction = binary_node.direction(dst);
                path.push(bool::from(next_direction));
                let next = binary_node.get_child(next_direction);
                match next {
                    NodeHandle::Hash(_) => {
                        let node = self.get_tree_branch_in_db_from_path(&path)?.ok_or(
                            BonsaiStorageError::Trie("Couldn't fetch node in db".to_string()),
                        )?;
                        self.latest_node_id.next_id();
                        self.storage_nodes.0.insert(self.latest_node_id, node);
                        nodes.push(self.latest_node_id);
                        match next_direction {
                            Direction::Left => {
                                binary_node.left = NodeHandle::InMemory(self.latest_node_id)
                            }
                            Direction::Right => {
                                binary_node.right = NodeHandle::InMemory(self.latest_node_id)
                            }
                        };
                        self.storage_nodes
                            .0
                            .insert(root_id, Node::Binary(binary_node));
                        self.preload_nodes_subtree(dst, self.latest_node_id, path, nodes)
                    }
                    NodeHandle::InMemory(next_id) => {
                        nodes.push(next_id);
                        self.preload_nodes_subtree(dst, next_id, path, nodes)
                    }
                }
            }
            Node::Edge(mut edge_node) if edge_node.path_matches(dst) => {
                path.extend_from_bitslice(&edge_node.path.0);
                if path == dst {
                    return Ok(());
                }
                let next = edge_node.child;
                match next {
                    NodeHandle::Hash(_) => {
                        let node = self.get_tree_branch_in_db_from_path(&path)?;
                        if let Some(node) = node {
                            self.latest_node_id.next_id();
                            self.storage_nodes.0.insert(self.latest_node_id, node);
                            nodes.push(self.latest_node_id);
                            edge_node.child = NodeHandle::InMemory(self.latest_node_id);
                            self.storage_nodes.0.insert(root_id, Node::Edge(edge_node));
                            self.preload_nodes_subtree(dst, self.latest_node_id, path, nodes)
                        } else {
                            Ok(())
                        }
                    }
                    NodeHandle::InMemory(next_id) => {
                        nodes.push(next_id);
                        self.preload_nodes_subtree(dst, next_id, path, nodes)
                    }
                }
            }
            Node::Edge(_) => Ok(()),
        }
    }

    fn get_tree_branch_in_db_from_path(
        &self,
        path: &BitVec<u8, Msb0>,
    ) -> Result<Option<Node>, BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
    {
        let key = if path.is_empty() {
            vec![]
        } else {
            [&[path.len() as u8], path.as_raw_slice()].concat()
        };
        self.db
            .get(&TrieKeyType::Trie(key))?
            .map(|node| {
                Node::decode(&mut node.as_slice()).map_err(|err| {
                    BonsaiStorageError::Trie(format!("Couldn't decode node: {}", err))
                })
            })
            .map_or(Ok(None), |r| r.map(Some))
    }

    /// This is a convenience function which merges the edge node with its child __iff__ it is also
    /// an edge.
    ///
    /// Does nothing if the child is not also an edge node.
    ///
    /// This can occur when mutating the tree (e.g. deleting a child of a binary node), and is an
    /// illegal state (since edge nodes __must be__ maximal subtrees).
    ///
    /// # Arguments
    ///
    /// * `parent` - The parent node to merge the child with.
    fn merge_edges(&self, parent: &mut EdgeNode) -> Result<(), BonsaiStorageError>
    where
        BonsaiStorageError: core::convert::From<<DB as BonsaiDatabase>::DatabaseError>,
    {
        //TODO: Add deletion of unused nodes
        let child_node = match parent.child {
            NodeHandle::Hash(_) => return Ok(()),
            NodeHandle::InMemory(child_id) => {
                self.storage_nodes
                    .0
                    .get(&child_id)
                    .ok_or(BonsaiStorageError::Trie(
                        "Couldn't fetch node in memory".to_string(),
                    ))?
            }
        };
        if let Node::Edge(child_edge) = child_node {
            parent.path.0.extend_from_bitslice(&child_edge.path.0);
            parent.child = child_edge.child;
        }
        Ok(())
    }

    #[cfg(test)]
    fn display(&self) {
        match self.root_handle {
            NodeHandle::Hash(hash) => {
                trace!("root is hash: {:?}", hash);
            }
            NodeHandle::InMemory(root_id) => {
                trace!("root is node: {:?}", root_id);
                self.print(&root_id);
            }
        }
    }

    #[cfg(test)]
    fn print(&self, head: &NodeId) {
        use Node::*;

        let current_tmp = self.storage_nodes.0.get(head).unwrap().clone();
        trace!("bonsai_node {:?} = {:?}", head, current_tmp);

        match current_tmp {
            Unresolved(hash) => {
                trace!("Unresolved: {:?}", hash);
            }
            Binary(binary) => {
                match &binary.get_child(Direction::Left) {
                    NodeHandle::Hash(hash) => {
                        trace!("left is hash {:?}", hash);
                    }
                    NodeHandle::InMemory(left_id) => {
                        self.print(left_id);
                    }
                }
                match &binary.get_child(Direction::Right) {
                    NodeHandle::Hash(hash) => {
                        trace!("right is hash {:?}", hash);
                    }
                    NodeHandle::InMemory(right_id) => {
                        self.print(right_id);
                    }
                }
            }
            Edge(edge) => match &edge.child {
                NodeHandle::Hash(hash) => {
                    trace!("child is hash {:?}", hash);
                }
                NodeHandle::InMemory(child_id) => {
                    self.print(child_id);
                }
            },
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        databases::{create_rocks_db, RocksDB, RocksDBConfig},
        id::BasicId,
        key_value_db::KeyValueDBConfig,
        KeyValueDB,
    };
    use bitvec::vec::BitVec;
    use mp_commitments::{calculate_class_commitment_leaf_hash, StateCommitmentTree};
    use mp_felt::Felt252Wrapper;
    use mp_hashers::pedersen::PedersenHasher;
    use parity_scale_codec::{Decode, Encode};
    use rand::prelude::*;
    use starknet_types_core::{felt::Felt, hash::Pedersen};

    // convert a Madara felt to a standard Felt
    fn felt_from_madara_felt(madara_felt: &Felt252Wrapper) -> Felt {
        let encoded = madara_felt.encode();
        Felt::decode(&mut &encoded[..]).unwrap()
    }

    // convert a standard Felt to a Madara felt
    fn madara_felt_from_felt(felt: &Felt) -> Felt252Wrapper {
        let encoded = felt.encode();
        Felt252Wrapper::decode(&mut &encoded[..]).unwrap()
    }

    #[test]
    fn one_commit_tree_compare() {
        let mut elements = vec![];
        let tempdir = tempfile::tempdir().unwrap();
        let mut rng = rand::thread_rng();
        let tree_size = rng.gen_range(10..100);
        for _ in 0..tree_size {
            let mut element = String::from("0x");
            let element_size = rng.gen_range(10..32);
            for _ in 0..element_size {
                let random_byte: u8 = rng.gen();
                element.push_str(&format!("{:02x}", random_byte));
            }
            elements.push(Felt::from_hex(&element).unwrap());
        }
        let madara_elements = elements
            .iter()
            .map(|felt| madara_felt_from_felt(felt))
            .collect::<Vec<_>>();
        let rocks_db = create_rocks_db(std::path::Path::new(tempdir.path())).unwrap();
        let rocks_db = RocksDB::new(&rocks_db, RocksDBConfig::default());
        let db = KeyValueDB::new(rocks_db, KeyValueDBConfig::default(), None);
        let mut bonsai_tree: super::MerkleTree<Pedersen, RocksDB<BasicId>, BasicId> =
            super::MerkleTree::new(db).unwrap();
        let root_hash = mp_commitments::calculate_class_commitment_tree_root_hash::<PedersenHasher>(
            &madara_elements,
        );
        elements
            .iter()
            .zip(madara_elements.iter())
            .for_each(|(element, madara_element)| {
                let final_hash =
                    calculate_class_commitment_leaf_hash::<PedersenHasher>(*madara_element);
                let key = &element.to_bytes_be()[..31];
                bonsai_tree
                    .set(
                        &BitVec::from_vec(key.to_vec()),
                        felt_from_madara_felt(&final_hash),
                    )
                    .unwrap();
            });
        bonsai_tree.display();
        assert_eq!(
            bonsai_tree.commit().unwrap(),
            felt_from_madara_felt(&root_hash)
        );
    }

    #[test]
    fn simple_commits() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut madara_tree = StateCommitmentTree::<PedersenHasher>::default();
        let rocks_db = create_rocks_db(std::path::Path::new(tempdir.path())).unwrap();
        let rocks_db = RocksDB::new(&rocks_db, RocksDBConfig::default());
        let db = KeyValueDB::new(rocks_db, KeyValueDBConfig::default(), None);
        let mut bonsai_tree: super::MerkleTree<Pedersen, RocksDB<BasicId>, BasicId> =
            super::MerkleTree::new(db).unwrap();
        let elements = [
            [Felt::from_hex("0x665342762FDD54D0303c195fec3ce2568b62052e").unwrap()],
            [Felt::from_hex("0x66342762FDD54D0303c195fec3ce2568b62052e").unwrap()],
            [Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap()],
        ];
        for elem in elements {
            elem.iter().for_each(|class_hash| {
                let final_hash =
                    felt_from_madara_felt(&calculate_class_commitment_leaf_hash::<PedersenHasher>(
                        madara_felt_from_felt(class_hash),
                    ));
                madara_tree.set(
                    madara_felt_from_felt(class_hash),
                    madara_felt_from_felt(&final_hash),
                );
                let key = &class_hash.to_bytes_be()[..31];
                bonsai_tree
                    .set(&BitVec::from_vec(key.to_vec()), final_hash)
                    .unwrap();
            });
        }
        let madara_root_hash = madara_tree.commit();
        let bonsai_root_hash = bonsai_tree.commit().unwrap();
        assert_eq!(bonsai_root_hash, felt_from_madara_felt(&madara_root_hash));
    }

    #[test]
    fn simple_commits_and_delete() {
        let tempdir = tempfile::tempdir().unwrap();
        let rocks_db = create_rocks_db(std::path::Path::new(tempdir.path())).unwrap();
        let rocks_db = RocksDB::new(&rocks_db, RocksDBConfig::default());
        let db = KeyValueDB::new(rocks_db, KeyValueDBConfig::default(), None);
        let mut bonsai_tree: super::MerkleTree<Pedersen, RocksDB<BasicId>, BasicId> =
            super::MerkleTree::new(db).unwrap();
        let elements = [
            [Felt::from_hex("0x665342762FDD54D0303c195fec3ce2568b62052e").unwrap()],
            [Felt::from_hex("0x66342762FDD54D0303c195fec3ce2568b62052e").unwrap()],
            [Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap()],
        ];
        for elem in elements {
            elem.iter().for_each(|class_hash| {
                let final_hash = calculate_class_commitment_leaf_hash::<PedersenHasher>(
                    madara_felt_from_felt(class_hash),
                );
                let key = &class_hash.to_bytes_be()[..31];
                bonsai_tree
                    .set(
                        &BitVec::from_vec(key.to_vec()),
                        felt_from_madara_felt(&final_hash),
                    )
                    .unwrap();
            });
        }
        bonsai_tree.commit().unwrap();
        for elem in elements {
            elem.iter().for_each(|class_hash| {
                let key = &class_hash.to_bytes_be()[..31];
                bonsai_tree
                    .set(&BitVec::from_vec(key.to_vec()), Felt::ZERO)
                    .unwrap();
            });
        }
        bonsai_tree.commit().unwrap();
    }

    #[test]
    fn multiple_commits_tree_compare() {
        let mut rng = rand::thread_rng();
        let tempdir = tempfile::tempdir().unwrap();
        let mut madara_tree = StateCommitmentTree::<PedersenHasher>::default();
        let rocks_db = create_rocks_db(std::path::Path::new(tempdir.path())).unwrap();
        let rocks_db = RocksDB::new(&rocks_db, RocksDBConfig::default());
        let db = KeyValueDB::new(rocks_db, KeyValueDBConfig::default(), None);
        let mut bonsai_tree: super::MerkleTree<Pedersen, RocksDB<BasicId>, BasicId> =
            super::MerkleTree::new(db).unwrap();
        let nb_commits = rng.gen_range(2..4);
        for _ in 0..nb_commits {
            let mut elements = vec![];
            let tree_size = rng.gen_range(10..100);
            for _ in 0..tree_size {
                let mut element = String::from("0x");
                let element_size = rng.gen_range(10..32);
                for _ in 0..element_size {
                    let random_byte: u8 = rng.gen();
                    element.push_str(&format!("{:02x}", random_byte));
                }
                elements.push(Felt::from_hex(&element).unwrap());
            }
            elements.iter().for_each(|class_hash| {
                let final_hash = calculate_class_commitment_leaf_hash::<PedersenHasher>(
                    madara_felt_from_felt(class_hash),
                );
                madara_tree.set(madara_felt_from_felt(class_hash), final_hash);
                let key = &class_hash.to_bytes_be()[..31];
                bonsai_tree
                    .set(
                        &BitVec::from_vec(key.to_vec()),
                        felt_from_madara_felt(&final_hash),
                    )
                    .unwrap();
            });

            let bonsai_root_hash = bonsai_tree.commit().unwrap();
            let madara_root_hash = madara_tree.commit();
            assert_eq!(bonsai_root_hash, felt_from_madara_felt(&madara_root_hash));
        }
    }
}
