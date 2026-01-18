use core::{fmt, marker::PhantomData};
use core::{iter, mem};
use parity_scale_codec::Decode;
use slotmap::SlotMap;
use starknet_types_core::{felt::Felt, hash::StarkHash};

use crate::trie::merkle_node::{hash_binary_node, hash_edge_node};
use crate::BitVec;
use crate::{
    error::BonsaiStorageError, format, hash_map, id::Id, vec, BitSlice, BonsaiDatabase, ByteVec,
    EncodeExt, HashMap, HashSet, KeyValueDB, ToString, Vec,
};

use super::iterator::MerkleTreeIterator;
use super::{
    merkle_node::{BinaryNode, Direction, EdgeNode, Node, NodeHandle},
    path::Path,
    trie_db::TrieKeyType,
    TrieKey,
};

#[cfg(test)]
use log::trace;

slotmap::new_key_type! {
    /// Key for an inmemory node.
    pub struct NodeKey;
}

// TODO: implement encode and decode by hand in Node
// these cases should never happen, otherwise that would mean we are saving in-memory node keys to the db, which would be very bad.
impl parity_scale_codec::Encode for NodeKey {
    fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, _f: F) -> R {
        unreachable!("Cannot encode NodeKey")
    }
}
impl parity_scale_codec::Decode for NodeKey {
    fn decode<I: parity_scale_codec::Input>(
        _input: &mut I,
    ) -> Result<Self, parity_scale_codec::Error> {
        unreachable!("Cannot decode NodeKey")
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RootHandle {
    Empty,
    Loaded(NodeKey),
}

/// A Starknet binary Merkle-Patricia tree with a specific root entry-point and storage.
///
/// This is used to update, mutate and access global Starknet state as well as individual contract
/// states.
///
/// For more information on how this functions internally, see [here](super::merkle_node).
pub struct MerkleTree<H: StarkHash> {
    /// The root node. None means the node has not been loaded yet.
    pub(crate) root_node: Option<RootHandle>,
    /// In-memory nodes.
    pub(crate) nodes: SlotMap<NodeKey, Node>,
    /// Identifier of the tree in the database.
    pub(crate) identifier: ByteVec,
    /// The list of nodes that should be removed from the underlying database during the next commit.
    pub(crate) death_row: HashSet<TrieKey>,
    /// The list of leaves that have been modified during the current commit.
    pub(crate) cache_leaf_modified: HashMap<ByteVec, InsertOrRemove<Felt>>,
    /// The maximum height of the tree. This is an u8 because we may rely on the fact that it's less than 256 in the future for optimizations.
    pub(crate) max_height: u8,
    /// The hasher used to hash the nodes.
    _hasher: PhantomData<H>,
}

impl<H: StarkHash> fmt::Debug for MerkleTree<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MerkleTree")
            .field("root_node", &self.root_node)
            .field("nodes", &self.nodes)
            .field("identifier", &self.identifier)
            .field("death_row", &self.death_row)
            .field("cache_leaf_modified", &self.cache_leaf_modified)
            .finish()
    }
}

// NB: #[derive(Clone)] does not work because it expands to an impl block which forces H: Clone, which Pedersen/Poseidon aren't.
#[cfg(feature = "bench")]
impl<H: StarkHash> Clone for MerkleTree<H> {
    fn clone(&self) -> Self {
        Self {
            max_height: self.max_height,
            root_node: self.root_node,
            nodes: self.nodes.clone(),
            identifier: self.identifier.clone(),
            death_row: self.death_row.clone(),
            cache_leaf_modified: self.cache_leaf_modified.clone(),
            _hasher: PhantomData,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InsertOrRemove<T> {
    Insert(T),
    Remove,
}
enum NodeOrFelt<'a> {
    Node(&'a Node),
    Felt(Felt),
}

impl<H: StarkHash + Send + Sync> MerkleTree<H> {
    pub fn new(identifier: ByteVec, max_height: u8) -> Self {
        Self {
            root_node: None,
            nodes: Default::default(),
            identifier,
            death_row: HashSet::new(),
            cache_leaf_modified: HashMap::new(),
            max_height,
            _hasher: PhantomData,
        }
    }

    /// Loads the root node or returns None if the tree is empty.
    pub(crate) fn load_root_node<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
    ) -> Result<Option<NodeKey>, BonsaiStorageError<DB::DatabaseError>> {
        // try_get_or_insert
        match self.root_node {
            Some(RootHandle::Loaded(id)) => Ok(Some(id)),
            Some(RootHandle::Empty) => Ok(None),
            None => {
                // load the node
                let id = self
                    .load_db_node(db, &TrieKey::new(&self.identifier, TrieKeyType::Trie, &[0]))?;

                match id {
                    Some(id) => {
                        self.root_node = Some(RootHandle::Loaded(id));
                        Ok(Some(id))
                    }
                    None => {
                        self.root_node = Some(RootHandle::Empty);
                        Ok(None)
                    }
                }
            }
        }
    }

    /// First step of two phase init.
    pub(crate) fn load_db_node<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
        key: &TrieKey,
    ) -> Result<Option<NodeKey>, BonsaiStorageError<DB::DatabaseError>> {
        if self.death_row.contains(key) {
            return Ok(None);
        }
        let node = db.get(key)?;
        let Some(node) = node else { return Ok(None) };

        let node = Node::decode(&mut node.as_slice())?;
        let key = self.nodes.insert(node);

        Ok(Some(key))
    }

    pub(crate) fn get_node_mut<DB: BonsaiDatabase>(
        &mut self,
        node_key: NodeKey,
    ) -> Result<&mut Node, BonsaiStorageError<DB::DatabaseError>> {
        self.nodes.get_mut(node_key).ok_or_else(|| {
            BonsaiStorageError::Trie(format!("Dangling in-memory node key: {node_key:?}"))
        })
    }

    pub(crate) fn load_node_handle<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
        handle: NodeHandle,
        path: &Path,
    ) -> Result<NodeKey, BonsaiStorageError<DB::DatabaseError>> {
        match handle {
            NodeHandle::Hash(_) => {
                // TODO(perf): useless allocs everywhere here...
                let path: ByteVec = path.clone().into();
                log::trace!("Visiting db node {:?}", path);
                let key = TrieKey::new(&self.identifier, TrieKeyType::Trie, &path);
                let Some(node_key) = self.load_db_node(db, &key)? else {
                    // Dangling node id in db
                    return Err(BonsaiStorageError::Trie(
                        "Could not get node from db".to_string(),
                    ));
                };
                Ok(node_key)
            }
            NodeHandle::InMemory(node_key) => Ok(node_key),
        }
    }

    /// Get or compute the hash of a node.
    pub(crate) fn get_or_compute_node_hash<DB: BonsaiDatabase>(
        &mut self,
        node: NodeHandle,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        match node {
            NodeHandle::Hash(felt) => Ok(felt),
            NodeHandle::InMemory(node_key) => {
                let computed_hash = match self.get_node_mut::<DB>(node_key)? {
                    Node::Binary(binary_node) => {
                        if let Some(hash) = binary_node.hash {
                            return Ok(hash);
                        }
                        let (left, right) = (binary_node.left, binary_node.right);
                        let left_hash = self.get_or_compute_node_hash::<DB>(left)?;
                        let right_hash = self.get_or_compute_node_hash::<DB>(right)?;
                        hash_binary_node::<H>(left_hash, right_hash)
                    }
                    Node::Edge(edge_node) => {
                        if let Some(hash) = edge_node.hash {
                            return Ok(hash);
                        }
                        let (path, child) = (edge_node.path.clone(), edge_node.child);
                        // edge_node borrow ends here
                        let child_hash = self.get_or_compute_node_hash::<DB>(child)?;
                        hash_edge_node::<H>(&path, child_hash)
                    }
                };

                // reborrow, for lifetime reasons (can't go into children if a borrow is alive)
                match self.get_node_mut::<DB>(node_key)? {
                    Node::Binary(binary_node) => binary_node.hash = Some(computed_hash),
                    Node::Edge(edge_node) => edge_node.hash = Some(computed_hash),
                }

                Ok(computed_hash)
            }
        }
    }

    /// Note: as iterators load nodes from the database, this takes an &mut self. However,
    /// note that it will not modify anything in the database - hence the &db.
    pub fn iter<'a, DB: BonsaiDatabase, ID: Id>(
        &'a mut self,
        db: &'a KeyValueDB<DB, ID>,
    ) -> MerkleTreeIterator<'a, H, DB, ID> {
        MerkleTreeIterator::new(self, db)
    }

    /// # Panics
    ///
    /// Calling this function when the tree has uncommited changes is invalid as the hashes need to be recomputed.
    pub fn root_hash<DB: BonsaiDatabase, ID: Id>(
        &self,
        db: &KeyValueDB<DB, ID>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        match self.root_node {
            Some(RootHandle::Empty) => Ok(Felt::ZERO),
            Some(RootHandle::Loaded(node_id)) => {
                let node = self.nodes.get(node_id).ok_or_else(|| {
                    BonsaiStorageError::Trie("Could not fetch root node from storage".into())
                })?;
                node.get_hash().ok_or_else(|| {
                    BonsaiStorageError::Trie("The tree has uncommited changes".into())
                })
            }
            None => {
                let Some(node) = Self::get_trie_branch_in_db_from_path(
                    &self.death_row,
                    &self.identifier,
                    db,
                    &Path::default(),
                )?
                else {
                    return Ok(Felt::ZERO);
                };
                Ok(node
                    .get_hash()
                    .expect("The fetched node has no computed hash"))
            }
        }
    }

    pub fn cache_leaf_modified(&self) -> &HashMap<ByteVec, InsertOrRemove<Felt>> {
        &self.cache_leaf_modified
    }

    /// Calculate all the new hashes and the root hash.
    #[allow(clippy::type_complexity)]
    pub(crate) fn get_updates<DB: BonsaiDatabase>(
        &mut self,
    ) -> Result<
        impl Iterator<Item = (TrieKey, InsertOrRemove<ByteVec>)>,
        BonsaiStorageError<DB::DatabaseError>,
    > {
        let mut updates = HashMap::new();
        for node_key in mem::take(&mut self.death_row) {
            updates.insert(node_key, InsertOrRemove::Remove);
        }

        if let Some(RootHandle::Loaded(node_id)) = &self.root_node {
            // compute hashes
            let mut hashes = vec![];
            self.compute_root_hash::<DB>(&mut hashes)?;

            // commit the tree
            self.commit_subtree::<DB>(
                &mut updates,
                *node_id,
                Path::default(),
                &mut hashes.into_iter(),
            )?;
        }

        self.root_node = None; // unloaded

        // Process leaf cache - parallel when std feature is enabled
        #[cfg(feature = "std")]
        {
            use rayon::prelude::*;
            let leaf_updates: Vec<_> = mem::take(&mut self.cache_leaf_modified)
                .into_par_iter()
                .map(|(key, value)| {
                    (
                        TrieKey::new(&self.identifier, TrieKeyType::Flat, &key),
                        match value {
                            InsertOrRemove::Insert(value) => {
                                InsertOrRemove::Insert(value.encode_bytevec())
                            }
                            InsertOrRemove::Remove => InsertOrRemove::Remove,
                        },
                    )
                })
                .collect();
            updates.extend(leaf_updates);
        }

        #[cfg(not(feature = "std"))]
        {
            for (key, value) in mem::take(&mut self.cache_leaf_modified) {
                updates.insert(
                    TrieKey::new(&self.identifier, TrieKeyType::Flat, &key),
                    match value {
                        InsertOrRemove::Insert(value) => {
                            InsertOrRemove::Insert(value.encode_bytevec())
                        }
                        InsertOrRemove::Remove => InsertOrRemove::Remove,
                    },
                );
            }
        }

        #[cfg(test)]
        self.assert_empty(); // we should have visited the whole tree

        Ok(updates.into_iter())
    }

    // Commit a single merkle tree
    #[cfg(test)]
    pub(crate) fn commit<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &mut KeyValueDB<DB, ID>,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        let db_changes = self.get_updates::<DB>()?;

        let mut batch = db.create_batch();
        for (key, value) in db_changes {
            match value {
                InsertOrRemove::Insert(value) => {
                    log::trace!("committing insert {:?} => {:?}", key, value);
                    db.insert(&key, &value, Some(&mut batch))?;
                }
                InsertOrRemove::Remove => {
                    log::trace!("committing remove {:?}", key);
                    db.remove(&key, Some(&mut batch))?;
                }
            }
        }
        db.write_batch(batch).unwrap();
        log::trace!("commit finished");

        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn assert_empty(&self) {
        // we don't use is_empty here for better error messages :)
        assert_eq!(self.nodes.iter().collect::<Vec<_>>(), vec![]);
    }

    fn get_node_or_felt<DB: BonsaiDatabase>(
        &self,
        node_handle: &NodeHandle,
    ) -> Result<NodeOrFelt, BonsaiStorageError<DB::DatabaseError>> {
        let node_id = match node_handle {
            NodeHandle::Hash(hash) => return Ok(NodeOrFelt::Felt(*hash)),
            NodeHandle::InMemory(node_id) => *node_id,
        };
        let node = self.nodes.get(node_id).ok_or(BonsaiStorageError::Trie(
            "Couldn't fetch node in the temporary storage".to_string(),
        ))?;
        Ok(NodeOrFelt::Node(node))
    }

    fn compute_root_hash<DB: BonsaiDatabase>(
        &self,
        hashes: &mut Vec<Felt>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        let handle = match &self.root_node {
            Some(RootHandle::Loaded(node_id)) => *node_id,
            Some(RootHandle::Empty) => return Ok(Felt::ZERO),
            None => {
                return Err(BonsaiStorageError::Trie(
                    "Root node is not loaded".to_string(),
                ))
            }
        };
        let Some(node) = self.nodes.get(handle) else {
            return Err(BonsaiStorageError::Trie(
                "Could not fetch root node from storage".to_string(),
            ));
        };
        self.compute_hashes::<DB>(node, Path::default(), hashes)
    }

    /// Compute the hashes of all of the updated nodes in the merkle tree. This step
    /// is separate from [`commit_subtree`] as it is done in parallel using rayon.
    /// Computed hashes are pushed to the `hashes` vector, depth first.
    fn compute_hashes<DB: BonsaiDatabase>(
        &self,
        node: &Node,
        path: Path,
        hashes: &mut Vec<Felt>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        use Node::*;

        match node {
            Binary(binary) => {
                // we check if we have one or two changed children

                let left_path = path.new_with_direction(Direction::Left);
                let node_left = self.get_node_or_felt::<DB>(&binary.left)?;
                let right_path = path.new_with_direction(Direction::Right);
                let node_right = self.get_node_or_felt::<DB>(&binary.right)?;

                let (left_hash, right_hash) = match (node_left, node_right) {
                    #[cfg(feature = "std")]
                    (NodeOrFelt::Node(left), NodeOrFelt::Node(right)) => {
                        // two children: use rayon
                        let (left, right) = rayon::join(
                            || self.compute_hashes::<DB>(left, left_path, hashes),
                            || {
                                let mut hashes = vec![];
                                let felt =
                                    self.compute_hashes::<DB>(right, right_path, &mut hashes)?;
                                Ok::<_, BonsaiStorageError<DB::DatabaseError>>((felt, hashes))
                            },
                        );
                        let (left_hash, (right_hash, hashes2)) = (left?, right?);
                        hashes.extend(hashes2);

                        (left_hash, right_hash)
                    }
                    (left, right) => {
                        let left_hash = match left {
                            NodeOrFelt::Felt(felt) => felt,
                            NodeOrFelt::Node(node) => {
                                self.compute_hashes::<DB>(node, left_path, hashes)?
                            }
                        };
                        let right_hash = match right {
                            NodeOrFelt::Felt(felt) => felt,
                            NodeOrFelt::Node(node) => {
                                self.compute_hashes::<DB>(node, right_path, hashes)?
                            }
                        };
                        (left_hash, right_hash)
                    }
                };

                let hash = hash_binary_node::<H>(left_hash, right_hash);

                hashes.push(hash);
                Ok(hash)
            }

            Edge(edge) => {
                let mut child_path = path.clone();
                child_path.0.extend(&edge.path.0);
                let child_hash = match self.get_node_or_felt::<DB>(&edge.child)? {
                    NodeOrFelt::Felt(felt) => felt,
                    NodeOrFelt::Node(node) => {
                        self.compute_hashes::<DB>(node, child_path, hashes)?
                    }
                };

                let hash = hash_edge_node::<H>(&edge.path, child_hash);
                hashes.push(hash);

                Ok(hash)
            }
        }
    }

    /// Persists any changes in this subtree to storage.
    ///
    /// This necessitates recursively calculating the hash of, and
    /// in turn persisting, any changed child nodes. This is necessary
    /// as the parent node's hash relies on its children hashes.
    /// Hash computation is done in parallel with [`compute_hashes`] beforehand.
    ///
    /// In effect, the entire tree gets persisted.
    ///
    /// # Arguments
    ///
    /// * `node_handle` - The top node from the subtree to commit.
    /// * `hashes` - The precomputed hashes for the subtree as returned by [`compute_hashes`].
    ///   The order is depth first, left to right.
    ///
    /// # Panics
    ///
    /// Panics if the precomputed `hashes` do not match the length of the modified subtree.
    #[cfg(feature = "std")]
    fn commit_subtree<DB: BonsaiDatabase>(
        &mut self,
        updates: &mut HashMap<TrieKey, InsertOrRemove<ByteVec>>,
        node_id: NodeKey,
        path: Path,
        hashes: &mut impl Iterator<Item = Felt>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        use rayon::prelude::*;

        // Phase 1: Collect all nodes with their paths and hashes (sequential traversal)
        let mut nodes_to_serialize: Vec<(ByteVec, Node)> = Vec::new();
        let root_hash =
            self.collect_nodes_for_commit::<DB>(node_id, path, hashes, &mut nodes_to_serialize)?;

        // Phase 2: Serialize nodes in parallel
        let serialized: Vec<_> = nodes_to_serialize
            .into_par_iter()
            .map(|(key_bytes, node)| {
                let trie_key = TrieKey::new(&self.identifier, TrieKeyType::Trie, &key_bytes);
                (trie_key, InsertOrRemove::Insert(node.encode_bytevec()))
            })
            .collect();

        updates.extend(serialized);
        Ok(root_hash)
    }

    /// Helper function to collect nodes during tree traversal.
    /// This separates the sequential traversal from the parallel serialization.
    #[cfg(feature = "std")]
    fn collect_nodes_for_commit<DB: BonsaiDatabase>(
        &mut self,
        node_id: NodeKey,
        path: Path,
        hashes: &mut impl Iterator<Item = Felt>,
        nodes_to_serialize: &mut Vec<(ByteVec, Node)>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        match self.nodes.remove(node_id).ok_or(BonsaiStorageError::Trie(
            "Couldn't fetch node in the temporary storage".to_string(),
        ))? {
            Node::Binary(mut binary) => {
                let left_path = path.new_with_direction(Direction::Left);
                let left_hash = match binary.left {
                    NodeHandle::Hash(left_hash) => left_hash,
                    NodeHandle::InMemory(node_id) => self.collect_nodes_for_commit::<DB>(
                        node_id,
                        left_path,
                        hashes,
                        nodes_to_serialize,
                    )?,
                };
                let right_path = path.new_with_direction(Direction::Right);
                let right_hash = match binary.right {
                    NodeHandle::Hash(right_hash) => right_hash,
                    NodeHandle::InMemory(node_id) => self.collect_nodes_for_commit::<DB>(
                        node_id,
                        right_path,
                        hashes,
                        nodes_to_serialize,
                    )?,
                };

                let hash = hashes.next().expect("mismatched hash state");

                binary.hash = Some(hash);
                binary.left = NodeHandle::Hash(left_hash);
                binary.right = NodeHandle::Hash(right_hash);
                let key_bytes: ByteVec = path.into();
                nodes_to_serialize.push((key_bytes, Node::Binary(binary)));
                Ok(hash)
            }
            Node::Edge(mut edge) => {
                let mut child_path = path.clone();
                child_path.0.extend(&edge.path.0);
                let child_hash = match edge.child {
                    NodeHandle::Hash(right_hash) => right_hash,
                    NodeHandle::InMemory(node_id) => self.collect_nodes_for_commit::<DB>(
                        node_id,
                        child_path,
                        hashes,
                        nodes_to_serialize,
                    )?,
                };
                let hash = hashes.next().expect("mismatched hash state");
                edge.hash = Some(hash);
                edge.child = NodeHandle::Hash(child_hash);
                let key_bytes: ByteVec = path.into();
                nodes_to_serialize.push((key_bytes, Node::Edge(edge)));
                Ok(hash)
            }
        }
    }

    /// Non-std version: sequential commit_subtree
    #[cfg(not(feature = "std"))]
    fn commit_subtree<DB: BonsaiDatabase>(
        &mut self,
        updates: &mut HashMap<TrieKey, InsertOrRemove<ByteVec>>,
        node_id: NodeKey,
        path: Path,
        hashes: &mut impl Iterator<Item = Felt>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        match self.nodes.remove(node_id).ok_or(BonsaiStorageError::Trie(
            "Couldn't fetch node in the temporary storage".to_string(),
        ))? {
            Node::Binary(mut binary) => {
                let left_path = path.new_with_direction(Direction::Left);
                let left_hash = match binary.left {
                    NodeHandle::Hash(left_hash) => left_hash,
                    NodeHandle::InMemory(node_id) => {
                        self.commit_subtree::<DB>(updates, node_id, left_path, hashes)?
                    }
                };
                let right_path = path.new_with_direction(Direction::Right);
                let right_hash = match binary.right {
                    NodeHandle::Hash(right_hash) => right_hash,
                    NodeHandle::InMemory(node_id) => {
                        self.commit_subtree::<DB>(updates, node_id, right_path, hashes)?
                    }
                };

                let hash = hashes.next().expect("mismatched hash state");

                binary.hash = Some(hash);
                binary.left = NodeHandle::Hash(left_hash);
                binary.right = NodeHandle::Hash(right_hash);
                let key_bytes: ByteVec = path.into();
                updates.insert(
                    TrieKey::new(&self.identifier, TrieKeyType::Trie, &key_bytes),
                    InsertOrRemove::Insert(Node::Binary(binary).encode_bytevec()),
                );
                Ok(hash)
            }
            Node::Edge(mut edge) => {
                let mut child_path = path.clone();
                child_path.0.extend(&edge.path.0);
                let child_hash = match edge.child {
                    NodeHandle::Hash(right_hash) => right_hash,
                    NodeHandle::InMemory(node_id) => {
                        self.commit_subtree::<DB>(updates, node_id, child_path, hashes)?
                    }
                };
                let hash = hashes.next().expect("mismatched hash state");
                edge.hash = Some(hash);
                edge.child = NodeHandle::Hash(child_hash);
                let key_bytes: ByteVec = path.into();
                updates.insert(
                    TrieKey::new(&self.identifier, TrieKeyType::Trie, &key_bytes),
                    InsertOrRemove::Insert(Node::Edge(edge).encode_bytevec()),
                );
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
    pub fn set<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
        key: &BitSlice,
        value: Felt,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        if value == Felt::ZERO {
            return self.delete_leaf(db, key);
        }
        if key.len() != self.max_height as _ {
            return Err(BonsaiStorageError::KeyLength {
                expected: self.max_height as _,
                got: key.len(),
            });
        }
        let key_bytes = bitslice_to_bytes(key);
        log::trace!("key_bytes: {:?}", key_bytes);

        // TODO(perf): do not double lookup when changing the value later (borrow needs to be split for preload_nodes though)
        let mut cache_leaf_entry = self.cache_leaf_modified.entry_ref(&key_bytes[..]);

        if let hash_map::EntryRef::Occupied(entry) = &mut cache_leaf_entry {
            if matches!(entry.get(), InsertOrRemove::Insert(_)) {
                entry.insert(InsertOrRemove::Insert(value));
                return Ok(());
            }
        }

        if let Some(value_db) = db.get(&TrieKey::new(
            &self.identifier,
            TrieKeyType::Flat,
            &key_bytes,
        ))? {
            if value == Felt::decode(&mut value_db.as_slice()).unwrap() {
                return Ok(());
            }
        }

        let mut iter = self.iter(db);
        iter.seek_to(key)?;
        log::trace!("Iter is {:?}", iter);
        let path_nodes = iter.current_nodes_heights;

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

        log::trace!("preload nodes: {:?}", path_nodes);
        use Node::*;
        match path_nodes.last() {
            Some((node_id, _)) => {
                let mut node = self.get_node_mut::<DB>(*node_id)?.clone();
                match &mut node {
                    Edge(edge) => {
                        let common = edge.common_path(key);
                        // Height of the binary node
                        let branch_height = edge.height as usize + common.len();
                        if branch_height == key.len() {
                            edge.child = NodeHandle::Hash(value);
                            // The leaf already exists, we simply change its value.
                            log::trace!("change val: {:?} => {:#x}", key_bytes, value);
                            self.cache_leaf_modified
                                .insert(key_bytes, InsertOrRemove::Insert(value));
                            self.nodes[*node_id] = node;
                            return Ok(());
                        }
                        // Height of the binary node's children
                        let child_height = branch_height + 1;

                        // Path from binary node to new leaf
                        let new_path = key[child_height..].to_bitvec();
                        // Path from binary node to existing child
                        let old_path = edge.path[common.len() + 1..].to_bitvec();

                        // The new leaf branch of the binary node.
                        // (this may be edge -> leaf, or just leaf depending).
                        log::trace!(
                            "cache_leaf_modified insert: {:?} => {:#x}",
                            key_bytes,
                            value
                        );
                        self.cache_leaf_modified
                            .insert(key_bytes, InsertOrRemove::Insert(value));

                        let new = if new_path.is_empty() {
                            NodeHandle::Hash(value)
                        } else {
                            let edge_id = self.nodes.insert(Node::Edge(EdgeNode {
                                hash: None,
                                height: child_height as u64,
                                path: Path(new_path),
                                child: NodeHandle::Hash(value),
                            }));
                            NodeHandle::InMemory(edge_id)
                        };

                        // The existing child branch of the binary node.
                        let old = if old_path.is_empty() {
                            edge.child
                        } else {
                            let edge_id = self.nodes.insert(Node::Edge(EdgeNode {
                                hash: None,
                                height: child_height as u64,
                                path: Path(old_path),
                                child: edge.child,
                            }));
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
                            let branch_id = self.nodes.insert(branch);
                            Node::Edge(EdgeNode {
                                hash: None,
                                height: edge.height,
                                path: Path(common.to_bitvec()),
                                child: NodeHandle::InMemory(branch_id),
                            })
                        };
                        let key_bytes = bitslice_to_bytes(&key[..edge.height as usize]);
                        log::trace!("2 death row add ({:?})", key_bytes);
                        self.death_row.insert(TrieKey::Trie(key_bytes));
                        node = new_node;
                    }
                    Binary(binary) => {
                        let child_height = binary.height + 1;

                        if child_height as usize == key.len() {
                            let direction = Direction::from(key[binary.height as usize]);
                            match direction {
                                Direction::Left => binary.left = NodeHandle::Hash(value),
                                Direction::Right => binary.right = NodeHandle::Hash(value),
                            };
                            self.cache_leaf_modified
                                .insert(key_bytes, InsertOrRemove::Insert(value));
                        }
                    }
                };

                // Update the node
                self.nodes[*node_id] = node;
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
                let node_id = self.nodes.insert(edge);
                self.root_node = Some(RootHandle::Loaded(node_id));

                let key_bytes = bitslice_to_bytes(key);
                self.cache_leaf_modified
                    .insert(key_bytes, InsertOrRemove::Insert(value));
                Ok(())
            }
        }
    }

    /// Deletes a leaf node from the tree.
    ///
    /// This is not an external facing API; the functionality is instead accessed by calling
    /// [`MerkleTree::set`] with value set to [`Felt::ZERO`].
    ///
    /// # Arguments
    ///
    /// * `key` - The key to delete.
    fn delete_leaf<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
        key: &BitSlice,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        if key.len() != self.max_height as _ {
            return Err(BonsaiStorageError::KeyLength {
                expected: self.max_height as _,
                got: key.len(),
            });
        }
        log::trace!("delete leaf");
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
        let key_bytes = bitslice_to_bytes(key);
        let leaf_entry = self.cache_leaf_modified.entry(key_bytes.clone());

        let tree_has_value = if let hash_map::Entry::Occupied(entry) = &leaf_entry {
            !matches!(entry.get(), InsertOrRemove::Remove)
        } else {
            db.get(&TrieKey::new(
                &self.identifier,
                TrieKeyType::Flat,
                &key_bytes,
            ))?
            .is_some()
        };

        if !tree_has_value {
            return Ok(());
        }
        leaf_entry.insert(InsertOrRemove::Remove);

        let mut iter = self.iter(db);
        iter.seek_to(key)?;
        log::trace!("Iter is {:?}", iter);
        let mut path_nodes = iter.current_nodes_heights;

        let mut last_binary_path = Path(key.to_bitvec());

        // Remove the final edge if present, we are starting from the closest binary node.
        if let Some((node_key, _height)) = path_nodes.last() {
            match self.get_node_mut::<DB>(*node_key)? {
                Node::Binary(_) => {}
                Node::Edge(edge) => {
                    // todo(perf) this is kinda dumb isnt it
                    for _ in 0..edge.path.len() {
                        last_binary_path.pop();
                    }
                    let mut new_path = Path(BitVec::new());
                    for i in last_binary_path.iter() {
                        new_path.push(*i);
                    }
                    last_binary_path = new_path.clone();
                    let path: ByteVec = (&last_binary_path).into();
                    log::trace!(
                        "iter leaf= edge={edge:?}, new_path={new_path:?}",
                        // TrieKey::new(self.identifier.clone(), TrieKeyType::Trie, &path)
                    );

                    self.death_row
                        .insert(TrieKey::new(&self.identifier, TrieKeyType::Trie, &path));
                    self.nodes.remove(*node_key);
                    path_nodes.pop();
                }
            }
        }

        let mut node_iter = path_nodes.into_iter().rev().peekable();

        let branch_node = node_iter.next();
        let parent_branch_node = node_iter.next();

        log::trace!(
            "remove leaf branch_node={branch_node:?} parent_branch_node={parent_branch_node:?}"
        );

        match branch_node {
            Some((node_id, _)) => {
                let (new_edge, par_path) = {
                    let node = self.get_node_mut::<DB>(node_id)?;

                    let binary = node
                        .as_binary()
                        .expect("The node must be a binary node due to the iteration condition");
                    let (direction, height) = { (binary.direction(key).invert(), binary.height) };
                    last_binary_path.pop();
                    last_binary_path.push(bool::from(direction));
                    // Create an edge node to replace the old binary node
                    // i.e. with the remaining child (note the direction invert),
                    //      and a path of just a single bit.
                    let path = Path(iter::once(bool::from(direction)).collect::<BitVec>());
                    let mut edge = EdgeNode {
                        hash: None,
                        height,
                        path,
                        child: match direction {
                            Direction::Left => binary.left,
                            Direction::Right => binary.right,
                        },
                    };

                    // Merge the remaining child if it's an edge.
                    self.merge_edges::<DB, ID>(&mut edge, db, &last_binary_path)?;
                    let cl = last_binary_path.clone();
                    last_binary_path.pop();
                    (edge, cl)
                };
                // Check the parent of the new edge. If it is also an edge, then they must merge.
                if let Some((parent_node_id, _)) = parent_branch_node {
                    // Get a mutable reference to the parent node to merge them
                    let parent_node = self.get_node_mut::<DB>(parent_node_id)?;
                    if let Node::Edge(parent_edge) = parent_node {
                        parent_edge.path.extend_from_bitslice(&new_edge.path.0);
                        parent_edge.child = new_edge.child;

                        let mut par_path = par_path;
                        par_path.pop();
                        let path: ByteVec = par_path.into();
                        self.death_row.insert(TrieKey::new(
                            &self.identifier,
                            TrieKeyType::Trie,
                            &path,
                        ));
                        self.nodes.remove(node_id);
                    } else {
                        self.nodes[node_id] = Node::Edge(new_edge);
                    }
                } else {
                    self.nodes[node_id] = Node::Edge(new_edge);
                }
            }
            None => {
                // We reached the root without a hitting binary node. The new tree
                // must therefore be empty.

                log::trace!("empty {:?}", self.root_node);
                if let Some(RootHandle::Loaded(node_id)) = self.root_node {
                    self.nodes.remove(node_id);
                }
                self.death_row
                    .insert(TrieKey::new(&self.identifier, TrieKeyType::Trie, &[0]));
                self.root_node = Some(RootHandle::Empty);
                return Ok(());
            }
        };
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
    pub fn get<DB: BonsaiDatabase, ID: Id>(
        &self,
        db: &KeyValueDB<DB, ID>,
        key: &BitSlice,
    ) -> Result<Option<Felt>, BonsaiStorageError<DB::DatabaseError>> {
        log::trace!("get with key {:b}", key);
        let key = bitslice_to_bytes(key);
        log::trace!("get from cache with {:?}", key);
        if let Some(value) = self.cache_leaf_modified.get(&key) {
            log::trace!("get has cache_leaf_modified {:?} {:?}", key, value);
            match value {
                InsertOrRemove::Remove => return Ok(None),
                InsertOrRemove::Insert(value) => return Ok(Some(*value)),
            }
        }
        log::trace!(
            "get from db with key {:?}",
            &TrieKey::new(&self.identifier, TrieKeyType::Flat, &key)
        );
        db.get(&TrieKey::new(&self.identifier, TrieKeyType::Flat, &key))
            .map(|r| r.map(|opt| Felt::decode(&mut opt.as_slice()).unwrap()))
    }

    pub fn get_at<DB: BonsaiDatabase, ID: Id>(
        &self,
        db: &KeyValueDB<DB, ID>,
        key: &BitSlice,
        id: ID,
    ) -> Result<Option<Felt>, BonsaiStorageError<DB::DatabaseError>> {
        let key = bitslice_to_bytes(key);
        db.get_at(&TrieKey::new(&self.identifier, TrieKeyType::Flat, &key), id)
            .map(|r| r.map(|opt| Felt::decode(&mut opt.as_slice()).unwrap()))
    }

    pub fn contains<DB: BonsaiDatabase, ID: Id>(
        &self,
        db: &KeyValueDB<DB, ID>,
        key: &BitSlice,
    ) -> Result<bool, BonsaiStorageError<DB::DatabaseError>> {
        let key = bitslice_to_bytes(key);
        if let Some(value) = self.cache_leaf_modified.get(&key) {
            match value {
                InsertOrRemove::Remove => return Ok(false),
                InsertOrRemove::Insert(_) => return Ok(true),
            }
        }
        db.contains(&TrieKey::new(&self.identifier, TrieKeyType::Flat, &key))
    }

    /// Get the node of the trie that corresponds to the path.
    fn get_trie_branch_in_db_from_path<DB: BonsaiDatabase, ID: Id>(
        death_row: &HashSet<TrieKey>,
        identifier: &[u8],
        db: &KeyValueDB<DB, ID>,
        path: &Path,
    ) -> Result<Option<Node>, BonsaiStorageError<DB::DatabaseError>> {
        log::trace!("getting: {:b}", path.0);

        let path: ByteVec = path.into();
        let key = TrieKey::new(identifier, TrieKeyType::Trie, &path);

        if death_row.contains(&key) {
            return Ok(None);
        }

        db.get(&key)?
            .map(|node| {
                log::trace!("got: {:?}", node);
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
    fn merge_edges<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        parent: &mut EdgeNode,
        db: &KeyValueDB<DB, ID>,
        path: &Path,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        match parent.child {
            NodeHandle::Hash(_) => {
                let node = Self::get_trie_branch_in_db_from_path(
                    &self.death_row,
                    &self.identifier,
                    db,
                    path,
                )?;
                log::trace!("case: Hash {:?}", node);
                if let Some(Node::Edge(child_edge)) = node {
                    parent.path.0.extend_from_bitslice(&child_edge.path.0);
                    parent.child = child_edge.child;
                    // remove node from db
                    let path: ByteVec = path.into();
                    log::trace!("4 death row {:?}", path);
                    self.death_row
                        .insert(TrieKey::new(&self.identifier, TrieKeyType::Trie, &path));
                }
            }
            NodeHandle::InMemory(child_id) => {
                let node = self.get_node_mut::<DB>(child_id)?;
                log::trace!("case: InMemory {:?}", node);

                if let Node::Edge(child_edge) = node {
                    parent.path.0.extend_from_bitslice(&child_edge.path.0);
                    parent.child = child_edge.child;

                    self.nodes.remove(child_id);

                    let path: ByteVec = path.into();
                    log::trace!("3 death row {:?}", path);
                    self.death_row
                        .insert(TrieKey::new(&self.identifier, TrieKeyType::Trie, &path));
                }
            }
        };
        Ok(())
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn dump(&self) {
        match self.root_node {
            Some(RootHandle::Empty) => {
                trace!("tree is empty")
            }
            Some(RootHandle::Loaded(node)) => {
                trace!("root is node {:?}", node);
                self.dump_node(node);
            }
            None => trace!("root is not loaded"),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    fn dump_node(&self, head: NodeKey) {
        use Node::*;

        let current_tmp = self.nodes[head].clone();
        trace!("bonsai_node {:?} = {:?}", head, current_tmp);

        match current_tmp {
            Binary(binary) => {
                match &binary.get_child(Direction::Left) {
                    NodeHandle::Hash(hash) => {
                        trace!("left is hash {:#x}", hash);
                    }
                    NodeHandle::InMemory(left_id) => {
                        self.dump_node(*left_id);
                    }
                }
                match &binary.get_child(Direction::Right) {
                    NodeHandle::Hash(hash) => {
                        trace!("right is hash {:#x}", hash);
                    }
                    NodeHandle::InMemory(right_id) => {
                        self.dump_node(*right_id);
                    }
                }
            }
            Edge(edge) => match &edge.child {
                NodeHandle::Hash(hash) => {
                    trace!("child is hash {:#x}", hash);
                }
                NodeHandle::InMemory(child_id) => {
                    self.dump_node(*child_id);
                }
            },
        };
    }
}

pub(crate) fn bitslice_to_bytes(bitslice: &BitSlice) -> ByteVec {
    // TODO(perf): this should not copy to a bitvec :(
    if bitslice.is_empty() {
        return Default::default();
    } // special case: tree root
    iter::once(bitslice.len() as u8)
        .chain(bitslice.to_bitvec().as_raw_slice().iter().copied())
        .collect()
}

pub(crate) fn bytes_to_bitvec(bytes: &[u8]) -> BitVec {
    BitSlice::from_slice(&bytes[1..]).to_bitvec()
}
