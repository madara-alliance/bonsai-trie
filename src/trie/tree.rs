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

use super::iterator::{MerkleTreeIterator, NodeVisitor};
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

#[derive(Debug, Default, Clone, Copy)]
struct TreePerfStats {
    db_node_loads: usize,
    in_memory_node_hits: usize,
}

#[derive(Debug)]
struct StagedHashComputation {
    root_hash: Felt,
    hashes: Vec<Felt>,
}

const RETAIN_FULL_FRONTIER_MIN_HOT_KEYS: usize = 512;
const RETAIN_FULL_FRONTIER_MAX_NODES: usize = 50_000;

#[cfg(feature = "std")]
#[derive(Default)]
struct StagedHashCacheCell(std::sync::Mutex<Option<StagedHashComputation>>);

#[cfg(feature = "std")]
impl fmt::Debug for StagedHashCacheCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let has_cached_hashes = self.lock_cache().is_some();
        f.debug_struct("StagedHashCacheCell")
            .field("has_cached_hashes", &has_cached_hashes)
            .finish()
    }
}

#[cfg(feature = "std")]
impl Clone for StagedHashCacheCell {
    fn clone(&self) -> Self {
        Self::default()
    }
}

#[cfg(feature = "std")]
impl StagedHashCacheCell {
    fn lock_cache(&self) -> std::sync::MutexGuard<'_, Option<StagedHashComputation>> {
        self.0.lock().unwrap_or_else(|poisoned| {
            log::warn!("recovering poisoned staged hash cache lock");
            poisoned.into_inner()
        })
    }

    fn clear(&self) {
        *self.lock_cache() = None;
    }

    fn cached_root_hash(&self) -> Option<Felt> {
        self.lock_cache().as_ref().map(|cached| cached.root_hash)
    }

    fn store(&self, computation: StagedHashComputation) {
        *self.lock_cache() = Some(computation);
    }

    fn take(&self) -> Option<StagedHashComputation> {
        self.lock_cache().take()
    }
}

#[cfg(not(feature = "std"))]
#[derive(Default, Debug, Clone)]
struct StagedHashCacheCell;

#[cfg(not(feature = "std"))]
impl StagedHashCacheCell {
    fn clear(&self) {}

    fn cached_root_hash(&self) -> Option<Felt> {
        None
    }

    fn store(&self, _computation: StagedHashComputation) {}

    fn take(&self) -> Option<StagedHashComputation> {
        None
    }
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
    /// Whether this tree has staged mutations that are not yet committed.
    dirty: bool,
    /// Cached staged root computation so commit can reuse the exact same hash walk.
    staged_hashes: StagedHashCacheCell,
    /// Per-block load counters to show whether the frontier stayed hot.
    perf_stats: TreePerfStats,
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
            .field("dirty", &self.dirty)
            .field("staged_hashes", &self.staged_hashes)
            .field("perf_stats", &self.perf_stats)
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
            dirty: self.dirty,
            staged_hashes: self.staged_hashes.clone(),
            perf_stats: self.perf_stats,
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

struct InvalidateHashesVisitor<H>(PhantomData<H>);

impl<H: StarkHash + Send + Sync> NodeVisitor<H> for InvalidateHashesVisitor<H> {
    fn visit_node<DB: BonsaiDatabase>(
        &mut self,
        tree: &mut MerkleTree<H>,
        node_id: NodeKey,
        _prev_height: usize,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        match tree.get_node_mut::<DB>(node_id)? {
            Node::Binary(binary_node) => binary_node.hash = None,
            Node::Edge(edge_node) => edge_node.hash = None,
        }
        Ok(())
    }
}

impl<H: StarkHash + Send + Sync> MerkleTree<H> {
    pub fn new(identifier: ByteVec, max_height: u8) -> Self {
        Self {
            root_node: None,
            nodes: Default::default(),
            identifier,
            death_row: HashSet::new(),
            cache_leaf_modified: HashMap::new(),
            dirty: false,
            staged_hashes: Default::default(),
            perf_stats: Default::default(),
            max_height,
            _hasher: PhantomData,
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.staged_hashes.clear();
    }

    fn in_memory_node_hash<DB: BonsaiDatabase>(
        &self,
        node_id: NodeKey,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        self.nodes
            .get(node_id)
            .and_then(Node::get_hash)
            .ok_or_else(|| {
                BonsaiStorageError::Trie(format!(
                    "missing committed hash for retained node {node_id:?}"
                ))
            })
    }

    fn mark_recent_frontier_nodes<DB: BonsaiDatabase>(
        &self,
        retained_nodes: &mut HashSet<NodeKey>,
        node_id: NodeKey,
        key: &BitSlice,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        let node = self.nodes.get(node_id).ok_or_else(|| {
            BonsaiStorageError::Trie(format!("Dangling in-memory node key: {node_id:?}"))
        })?;
        retained_nodes.insert(node_id);

        match node {
            Node::Binary(binary) => {
                let height = binary.height as usize;
                if height >= key.len() {
                    return Ok(());
                }

                if let NodeHandle::InMemory(child_id) =
                    binary.get_child(Direction::from(key[height]))
                {
                    self.mark_recent_frontier_nodes::<DB>(retained_nodes, child_id, key)?;
                }
            }
            Node::Edge(edge) => {
                let height = edge.height as usize;
                if height >= key.len() {
                    return Ok(());
                }

                let key_suffix = &key[height..];
                let common_len = key_suffix
                    .iter()
                    .zip(edge.path.0.iter())
                    .take_while(|(lhs, rhs)| lhs == rhs)
                    .count();
                if common_len != edge.path.len() {
                    return Ok(());
                }

                if let NodeHandle::InMemory(child_id) = edge.child {
                    self.mark_recent_frontier_nodes::<DB>(retained_nodes, child_id, key)?;
                }
            }
        }

        Ok(())
    }

    fn retain_child_handle<DB: BonsaiDatabase>(
        &self,
        handle: NodeHandle,
        retained_nodes: &HashSet<NodeKey>,
        remapped_nodes: &mut HashMap<NodeKey, NodeKey>,
        new_nodes: &mut SlotMap<NodeKey, Node>,
    ) -> Result<NodeHandle, BonsaiStorageError<DB::DatabaseError>> {
        match handle {
            NodeHandle::Hash(hash) => Ok(NodeHandle::Hash(hash)),
            NodeHandle::InMemory(node_id) if retained_nodes.contains(&node_id) => {
                Ok(NodeHandle::InMemory(self.rebuild_retained_frontier::<DB>(
                    node_id,
                    retained_nodes,
                    remapped_nodes,
                    new_nodes,
                )?))
            }
            NodeHandle::InMemory(node_id) => {
                Ok(NodeHandle::Hash(self.in_memory_node_hash::<DB>(node_id)?))
            }
        }
    }

    fn rebuild_retained_frontier<DB: BonsaiDatabase>(
        &self,
        node_id: NodeKey,
        retained_nodes: &HashSet<NodeKey>,
        remapped_nodes: &mut HashMap<NodeKey, NodeKey>,
        new_nodes: &mut SlotMap<NodeKey, Node>,
    ) -> Result<NodeKey, BonsaiStorageError<DB::DatabaseError>> {
        if let Some(remapped_id) = remapped_nodes.get(&node_id).copied() {
            return Ok(remapped_id);
        }

        let mut node = self.nodes.get(node_id).cloned().ok_or_else(|| {
            BonsaiStorageError::Trie(format!("Dangling in-memory node key: {node_id:?}"))
        })?;

        match &mut node {
            Node::Binary(binary) => {
                binary.left = self.retain_child_handle::<DB>(
                    binary.left,
                    retained_nodes,
                    remapped_nodes,
                    new_nodes,
                )?;
                binary.right = self.retain_child_handle::<DB>(
                    binary.right,
                    retained_nodes,
                    remapped_nodes,
                    new_nodes,
                )?;
            }
            Node::Edge(edge) => {
                edge.child = self.retain_child_handle::<DB>(
                    edge.child,
                    retained_nodes,
                    remapped_nodes,
                    new_nodes,
                )?;
            }
        }

        let remapped_id = new_nodes.insert(node);
        remapped_nodes.insert(node_id, remapped_id);
        Ok(remapped_id)
    }

    fn retain_recent_frontier<DB: BonsaiDatabase>(
        &mut self,
        hot_keys: &[ByteVec],
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        let Some(root_handle) = self.root_node else {
            self.nodes.clear();
            return Ok(());
        };
        let RootHandle::Loaded(root_id) = root_handle else {
            self.nodes.clear();
            return Ok(());
        };

        let retained_before = self.nodes.len();
        if hot_keys.len() >= RETAIN_FULL_FRONTIER_MIN_HOT_KEYS
            && retained_before <= RETAIN_FULL_FRONTIER_MAX_NODES
        {
            log::debug!(
                "bonsai retained frontier kept_full identifier={:?} hot_keys={} retained_nodes={} min_hot_keys={} max_nodes={}",
                self.identifier,
                hot_keys.len(),
                retained_before,
                RETAIN_FULL_FRONTIER_MIN_HOT_KEYS,
                RETAIN_FULL_FRONTIER_MAX_NODES,
            );
            return Ok(());
        }

        let mut retained_nodes = HashSet::default();
        retained_nodes.insert(root_id);
        for key in hot_keys {
            self.mark_recent_frontier_nodes::<DB>(&mut retained_nodes, root_id, hot_key_bits(key))?;
        }

        let mut remapped_nodes = HashMap::default();
        let mut new_nodes = SlotMap::default();
        let new_root = self.rebuild_retained_frontier::<DB>(
            root_id,
            &retained_nodes,
            &mut remapped_nodes,
            &mut new_nodes,
        )?;

        self.nodes = new_nodes;
        self.root_node = Some(RootHandle::Loaded(new_root));
        log::debug!(
            "bonsai retained frontier compacted identifier={:?} hot_keys={} retained_before={} retained_after={}",
            self.identifier,
            hot_keys.len(),
            retained_before,
            self.nodes.len(),
        );

        Ok(())
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
        self.perf_stats.db_node_loads += 1;

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
            NodeHandle::InMemory(node_key) => {
                self.perf_stats.in_memory_node_hits += 1;
                Ok(node_key)
            }
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
    ) -> Result<HashMap<TrieKey, InsertOrRemove<ByteVec>>, BonsaiStorageError<DB::DatabaseError>>
    {
        let dirty_before_commit = self.dirty;
        let hot_keys = self.cache_leaf_modified.keys().cloned().collect::<Vec<_>>();
        let mut updates = HashMap::new();
        for node_key in mem::take(&mut self.death_row) {
            updates.insert(node_key, InsertOrRemove::Remove);
        }

        let mut used_staged_hash_cache = false;
        let mut precomputed_hashes = 0usize;
        if self.dirty {
            if let Some(RootHandle::Loaded(node_id)) = self.root_node {
                let hashes = if let Some(staged) = self.staged_hashes.take() {
                    used_staged_hash_cache = true;
                    precomputed_hashes = staged.hashes.len();
                    staged.hashes
                } else {
                    let mut hashes = vec![];
                    self.compute_root_hash::<DB>(&mut hashes)?;
                    precomputed_hashes = hashes.len();
                    hashes
                };

                self.commit_subtree_cached::<DB>(
                    &mut updates,
                    node_id,
                    Path::default(),
                    &mut hashes.into_iter(),
                )?;
            }
            self.dirty = false;
            self.retain_recent_frontier::<DB>(&hot_keys)?;
        }

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
        log::debug!(
            "bonsai commit identifier={:?} dirty_before_commit={} used_staged_hash_cache={} precomputed_hashes={} db_node_loads={} in_memory_node_hits={} retained_nodes={} root_loaded={} flat_changes={} trie_updates={}",
            self.identifier,
            dirty_before_commit,
            used_staged_hash_cache,
            precomputed_hashes,
            self.perf_stats.db_node_loads,
            self.perf_stats.in_memory_node_hits,
            self.nodes.len(),
            matches!(self.root_node, Some(RootHandle::Loaded(_))),
            updates.keys().filter(|key| matches!(key, TrieKey::Flat(_))).count(),
            updates.keys().filter(|key| matches!(key, TrieKey::Trie(_))).count(),
        );
        self.perf_stats = Default::default();

        Ok(updates)
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

    /// Compute the root hash from staged (uncommitted) in-memory changes without
    /// persisting anything. If no staged changes exist, falls back to reading the
    /// committed root from the database.
    pub(crate) fn root_hash_staged<DB: BonsaiDatabase, ID: Id>(
        &self,
        db: &KeyValueDB<DB, ID>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        if !self.dirty {
            return self.root_hash(db);
        }

        if let Some(root_hash) = self.staged_hashes.cached_root_hash() {
            log::debug!(
                "bonsai staged root cache hit identifier={:?} retained_nodes={}",
                self.identifier,
                self.nodes.len(),
            );
            return Ok(root_hash);
        }

        match &self.root_node {
            Some(RootHandle::Loaded(_)) => {
                let mut hashes = vec![];
                let root_hash = self.compute_root_hash::<DB>(&mut hashes)?;
                log::debug!(
                    "bonsai staged root computed identifier={:?} precomputed_hashes={} retained_nodes={} db_node_loads={} in_memory_node_hits={}",
                    self.identifier,
                    hashes.len(),
                    self.nodes.len(),
                    self.perf_stats.db_node_loads,
                    self.perf_stats.in_memory_node_hits,
                );
                self.staged_hashes
                    .store(StagedHashComputation { root_hash, hashes });
                Ok(root_hash)
            }
            Some(RootHandle::Empty) => Ok(Felt::ZERO),
            None => self.root_hash(db),
        }
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
        if let Some(hash) = node.get_hash() {
            return Ok(hash);
        }

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
    fn commit_subtree_cached<DB: BonsaiDatabase>(
        &mut self,
        updates: &mut HashMap<TrieKey, InsertOrRemove<ByteVec>>,
        node_id: NodeKey,
        path: Path,
        hashes: &mut impl Iterator<Item = Felt>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        let mut nodes_to_serialize = Vec::new();
        let root_hash = self.collect_nodes_for_commit_cached::<DB>(
            node_id,
            path,
            hashes,
            &mut nodes_to_serialize,
        )?;

        #[cfg(feature = "std")]
        {
            use rayon::prelude::*;

            let serialized: Vec<_> = nodes_to_serialize
                .into_par_iter()
                .map(|(key_bytes, node)| {
                    (
                        TrieKey::new(&self.identifier, TrieKeyType::Trie, &key_bytes),
                        InsertOrRemove::Insert(node.encode_bytevec()),
                    )
                })
                .collect();
            updates.extend(serialized);
        }

        #[cfg(not(feature = "std"))]
        {
            for (key_bytes, node) in nodes_to_serialize {
                updates.insert(
                    TrieKey::new(&self.identifier, TrieKeyType::Trie, &key_bytes),
                    InsertOrRemove::Insert(node.encode_bytevec()),
                );
            }
        }

        Ok(root_hash)
    }

    fn collect_nodes_for_commit_cached<DB: BonsaiDatabase>(
        &mut self,
        node_id: NodeKey,
        path: Path,
        hashes: &mut impl Iterator<Item = Felt>,
        nodes_to_serialize: &mut Vec<(ByteVec, Node)>,
    ) -> Result<Felt, BonsaiStorageError<DB::DatabaseError>> {
        if let Some(hash) = self.nodes.get(node_id).and_then(Node::get_hash) {
            return Ok(hash);
        }

        match self
            .nodes
            .get(node_id)
            .cloned()
            .ok_or(BonsaiStorageError::Trie(
                "Couldn't fetch node in the temporary storage".to_string(),
            ))? {
            Node::Binary(binary) => {
                let left_path = path.new_with_direction(Direction::Left);
                let left_hash = match binary.left {
                    NodeHandle::Hash(left_hash) => left_hash,
                    NodeHandle::InMemory(node_id) => self.collect_nodes_for_commit_cached::<DB>(
                        node_id,
                        left_path,
                        hashes,
                        nodes_to_serialize,
                    )?,
                };
                let right_path = path.new_with_direction(Direction::Right);
                let right_hash = match binary.right {
                    NodeHandle::Hash(right_hash) => right_hash,
                    NodeHandle::InMemory(node_id) => self.collect_nodes_for_commit_cached::<DB>(
                        node_id,
                        right_path,
                        hashes,
                        nodes_to_serialize,
                    )?,
                };

                let hash = hashes.next().expect("mismatched hash state");

                match self.get_node_mut::<DB>(node_id)? {
                    Node::Binary(binary_node) => binary_node.hash = Some(hash),
                    Node::Edge(_) => unreachable!("node changed type while committing"),
                }

                let mut persisted_binary = binary;
                persisted_binary.hash = Some(hash);
                persisted_binary.left = NodeHandle::Hash(left_hash);
                persisted_binary.right = NodeHandle::Hash(right_hash);
                let key_bytes: ByteVec = path.into();
                nodes_to_serialize.push((key_bytes, Node::Binary(persisted_binary)));
                Ok(hash)
            }
            Node::Edge(edge) => {
                let mut child_path = path.clone();
                child_path.0.extend(&edge.path.0);
                let child_hash = match edge.child {
                    NodeHandle::Hash(right_hash) => right_hash,
                    NodeHandle::InMemory(node_id) => self.collect_nodes_for_commit_cached::<DB>(
                        node_id,
                        child_path,
                        hashes,
                        nodes_to_serialize,
                    )?,
                };
                let hash = hashes.next().expect("mismatched hash state");

                match self.get_node_mut::<DB>(node_id)? {
                    Node::Edge(edge_node) => edge_node.hash = Some(hash),
                    Node::Binary(_) => unreachable!("node changed type while committing"),
                }

                let mut persisted_edge = edge;
                persisted_edge.hash = Some(hash);
                persisted_edge.child = NodeHandle::Hash(child_hash);
                let key_bytes: ByteVec = path.into();
                nodes_to_serialize.push((key_bytes, Node::Edge(persisted_edge)));
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
        let key_bytes = bitslice_to_bytes(key);
        self.set_with_key_bytes(db, key, key_bytes, value, true)
    }

    pub fn set_owned<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
        key: BitVec,
        value: Felt,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        let key_bytes = bitvec_to_bytes(&key);
        self.set_with_key_bytes(db, &key, key_bytes, value, true)
    }

    pub fn set_many_owned<DB, ID, I>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
        entries: I,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>>
    where
        DB: BonsaiDatabase,
        ID: Id,
        I: IntoIterator<Item = (BitVec, Felt)>,
    {
        let entries = entries.into_iter().collect::<Vec<_>>();
        if entries.iter().any(|(_, value)| *value == Felt::ZERO) {
            for (key, value) in entries {
                let key_bytes = bitvec_to_bytes(&key);
                self.set_with_key_bytes(db, &key, key_bytes, value, true)?;
            }
            return Ok(());
        }

        let entries = self.prepare_nonzero_bulk_entries::<DB>(entries)?;
        let mut iter = self.iter(db);
        for (key, key_bytes, value) in entries {
            Self::set_nonzero_with_key_bytes_using_iter(&mut iter, &key, key_bytes, value, true)?;
        }
        Ok(())
    }

    pub fn set_many_owned_assume_changed<DB, ID, I>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
        entries: I,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>>
    where
        DB: BonsaiDatabase,
        ID: Id,
        I: IntoIterator<Item = (BitVec, Felt)>,
    {
        let entries = entries.into_iter().collect::<Vec<_>>();
        if entries.iter().any(|(_, value)| *value == Felt::ZERO) {
            for (key, value) in entries {
                let key_bytes = bitvec_to_bytes(&key);
                self.set_with_key_bytes(db, &key, key_bytes, value, false)?;
            }
            return Ok(());
        }

        let entries = self.prepare_nonzero_bulk_entries::<DB>(entries)?;
        let mut iter = self.iter(db);
        for (key, key_bytes, value) in entries {
            Self::set_nonzero_with_key_bytes_using_iter(&mut iter, &key, key_bytes, value, false)?;
        }
        Ok(())
    }

    fn prepare_nonzero_bulk_entries<DB: BonsaiDatabase>(
        &self,
        entries: Vec<(BitVec, Felt)>,
    ) -> Result<Vec<(BitVec, ByteVec, Felt)>, BonsaiStorageError<DB::DatabaseError>> {
        let mut prepared = Vec::with_capacity(entries.len());
        for (position, (key, value)) in entries.into_iter().enumerate() {
            if key.len() != usize::from(self.max_height) {
                return Err(BonsaiStorageError::KeyLength {
                    expected: usize::from(self.max_height),
                    got: key.len(),
                });
            }
            let key_bytes = bitvec_to_bytes(&key);
            prepared.push((key_bytes, position, key, value));
        }

        prepared.sort_unstable_by(|lhs, rhs| lhs.0.cmp(&rhs.0).then(lhs.1.cmp(&rhs.1)));

        let mut deduped = Vec::with_capacity(prepared.len());
        for (key_bytes, _position, key, value) in prepared {
            if let Some((last_key, last_key_bytes, last_value)) = deduped.last_mut() {
                if *last_key_bytes == key_bytes {
                    *last_key = key;
                    *last_value = value;
                    continue;
                }
            }
            deduped.push((key, key_bytes, value));
        }

        Ok(deduped)
    }

    fn set_nonzero_with_key_bytes_using_iter<DB: BonsaiDatabase, ID: Id>(
        iter: &mut MerkleTreeIterator<'_, H, DB, ID>,
        key: &BitSlice,
        key_bytes: ByteVec,
        value: Felt,
        check_committed_value: bool,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        debug_assert_ne!(value, Felt::ZERO);
        if key.len() != usize::from(iter.tree.max_height) {
            return Err(BonsaiStorageError::KeyLength {
                expected: usize::from(iter.tree.max_height),
                got: key.len(),
            });
        }
        log::trace!("key_bytes: {:?}", key_bytes);
        let has_staged_override = match iter.tree.cache_leaf_modified.get(&key_bytes) {
            Some(InsertOrRemove::Insert(staged_value)) if *staged_value == value => return Ok(()),
            Some(_) => true,
            None => false,
        };

        if check_committed_value && !has_staged_override {
            if let Some(value_db) = iter.db.get(&TrieKey::new(
                &iter.tree.identifier,
                TrieKeyType::Flat,
                &key_bytes,
            ))? {
                if value == Felt::decode(&mut value_db.as_slice()).unwrap() {
                    return Ok(());
                }
            }
        }

        iter.tree.mark_dirty();
        iter.traverse_to(&mut InvalidateHashesVisitor(PhantomData), key)?;
        log::trace!("Iter is {:?}", iter);
        let path_nodes = iter.current_nodes_heights.clone();

        log::trace!("preload nodes: {:?}", path_nodes);
        use Node::*;
        match path_nodes.last() {
            Some((node_id, _)) => {
                let tree = &mut *iter.tree;
                let mut node = tree.get_node_mut::<DB>(*node_id)?.clone();
                match &mut node {
                    Edge(edge) => {
                        let common = edge.common_path(key);
                        let branch_height = edge.height as usize + common.len();
                        if branch_height == key.len() {
                            edge.child = NodeHandle::Hash(value);
                            log::trace!("change val: {:?} => {:#x}", key_bytes, value);
                            tree.cache_leaf_modified
                                .insert(key_bytes, InsertOrRemove::Insert(value));
                            tree.nodes[*node_id] = node;
                            return Ok(());
                        }

                        let child_height = branch_height + 1;
                        let new_path = key[child_height..].to_bitvec();
                        let old_path = edge.path[common.len() + 1..].to_bitvec();

                        log::trace!(
                            "cache_leaf_modified insert: {:?} => {:#x}",
                            key_bytes,
                            value
                        );
                        tree.cache_leaf_modified
                            .insert(key_bytes, InsertOrRemove::Insert(value));

                        let new = if new_path.is_empty() {
                            NodeHandle::Hash(value)
                        } else {
                            let edge_id = tree.nodes.insert(Node::Edge(EdgeNode {
                                hash: None,
                                height: child_height as u64,
                                path: Path(new_path),
                                child: NodeHandle::Hash(value),
                            }));
                            NodeHandle::InMemory(edge_id)
                        };

                        let old = if old_path.is_empty() {
                            edge.child
                        } else {
                            let edge_id = tree.nodes.insert(Node::Edge(EdgeNode {
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

                        let new_node = if common.is_empty() {
                            branch
                        } else {
                            let branch_id = tree.nodes.insert(branch);
                            Node::Edge(EdgeNode {
                                hash: None,
                                height: edge.height,
                                path: Path(common.to_bitvec()),
                                child: NodeHandle::InMemory(branch_id),
                            })
                        };
                        let key_bytes = bitslice_to_bytes(&key[..edge.height as usize]);
                        log::trace!("2 death row add ({:?})", key_bytes);
                        tree.death_row.insert(TrieKey::Trie(key_bytes));
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
                            tree.cache_leaf_modified
                                .insert(key_bytes, InsertOrRemove::Insert(value));
                        }
                    }
                };

                tree.nodes[*node_id] = node;
                Ok(())
            }
            None => {
                let edge = Node::Edge(EdgeNode {
                    hash: None,
                    height: 0,
                    path: Path(key.to_bitvec()),
                    child: NodeHandle::Hash(value),
                });
                let node_id = iter.tree.nodes.insert(edge);
                iter.tree.root_node = Some(RootHandle::Loaded(node_id));
                iter.current_path = Path(key.to_bitvec());
                iter.current_nodes_heights.clear();
                iter.current_nodes_heights.push((node_id, 0));

                iter.tree
                    .cache_leaf_modified
                    .insert(key_bytes, InsertOrRemove::Insert(value));
                Ok(())
            }
        }
    }

    fn set_with_key_bytes<DB: BonsaiDatabase, ID: Id>(
        &mut self,
        db: &KeyValueDB<DB, ID>,
        key: &BitSlice,
        key_bytes: ByteVec,
        value: Felt,
        check_committed_value: bool,
    ) -> Result<(), BonsaiStorageError<DB::DatabaseError>> {
        if value == Felt::ZERO {
            return self.delete_leaf(db, key);
        }
        if key.len() != usize::from(self.max_height) {
            return Err(BonsaiStorageError::KeyLength {
                expected: usize::from(self.max_height),
                got: key.len(),
            });
        }
        log::trace!("key_bytes: {:?}", key_bytes);
        let has_staged_override = match self.cache_leaf_modified.get(&key_bytes) {
            Some(InsertOrRemove::Insert(staged_value)) if *staged_value == value => return Ok(()),
            Some(_) => true,
            None => false,
        };

        if check_committed_value && !has_staged_override {
            if let Some(value_db) = db.get(&TrieKey::new(
                &self.identifier,
                TrieKeyType::Flat,
                &key_bytes,
            ))? {
                if value == Felt::decode(&mut value_db.as_slice()).unwrap() {
                    return Ok(());
                }
            }
        }

        self.mark_dirty();
        let mut iter = self.iter(db);
        iter.traverse_to(&mut InvalidateHashesVisitor(PhantomData), key)?;
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
        if key.len() != usize::from(self.max_height) {
            return Err(BonsaiStorageError::KeyLength {
                expected: usize::from(self.max_height),
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

        self.mark_dirty();
        let mut iter = self.iter(db);
        iter.traverse_to(&mut InvalidateHashesVisitor(PhantomData), key)?;
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

fn bitvec_to_bytes(bitvec: &BitVec) -> ByteVec {
    if bitvec.is_empty() {
        return Default::default();
    }
    iter::once(bitvec.len() as u8)
        .chain(bitvec.as_raw_slice().iter().copied())
        .collect()
}

pub(crate) fn bytes_to_bitvec(bytes: &[u8]) -> BitVec {
    BitSlice::from_slice(&bytes[1..]).to_bitvec()
}

fn hot_key_bits(key: &[u8]) -> &BitSlice {
    let Some((&bit_len, raw_bits)) = key.split_first() else {
        return BitSlice::empty();
    };
    let bits = BitSlice::from_slice(raw_bits);
    &bits[..usize::from(bit_len).min(bits.len())]
}

#[cfg(all(test, feature = "std"))]
mod staged_hash_cache_tests {
    use super::{Node, NodeHandle, RootHandle, StagedHashCacheCell, StagedHashComputation};
    use crate::{
        databases::HashMapDb,
        id::{BasicId, BasicIdBuilder},
        BitVec, BonsaiStorage, BonsaiStorageConfig,
    };
    use starknet_types_core::felt::Felt;
    use starknet_types_core::hash::Pedersen;

    fn retained_tree<'a>(
        storage: &'a BonsaiStorage<BasicId, HashMapDb<BasicId>, Pedersen>,
        identifier: &'a [u8],
    ) -> &'a super::MerkleTree<Pedersen> {
        storage.tries.trees.get(identifier).unwrap()
    }

    #[test]
    fn staged_hash_cache_recovers_from_poison() {
        let cache = StagedHashCacheCell::default();

        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = cache.0.lock().unwrap();
            panic!("poison staged hash cache");
        }));

        cache.store(StagedHashComputation {
            root_hash: Felt::ONE,
            hashes: vec![Felt::TWO],
        });
        assert_eq!(cache.cached_root_hash(), Some(Felt::ONE));
        assert_eq!(
            cache.take().map(|cached| cached.hashes),
            Some(vec![Felt::TWO])
        );
    }

    #[test]
    fn retained_frontier_prunes_unmodified_branch_after_commit() {
        let identifier = vec![];
        let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> = BonsaiStorage::new(
            HashMapDb::<BasicId>::default(),
            BonsaiStorageConfig::default(),
            8,
        );
        let mut id_builder = BasicIdBuilder::new();

        bonsai_storage
            .insert(
                &identifier,
                &BitVec::from_vec(vec![0b0000_0000]),
                &Felt::from_hex("0x11").unwrap(),
            )
            .unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();

        bonsai_storage
            .insert(
                &identifier,
                &BitVec::from_vec(vec![0b1000_0000]),
                &Felt::from_hex("0x22").unwrap(),
            )
            .unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();

        let tree = retained_tree(&bonsai_storage, &identifier);
        let root_id = match tree.root_node {
            Some(RootHandle::Loaded(root_id)) => root_id,
            other => panic!("expected loaded root, got {other:?}"),
        };
        let root = tree.nodes.get(root_id).unwrap();
        let Node::Binary(root) = root else {
            panic!("expected binary root after inserting divergent keys");
        };

        assert_eq!(tree.nodes.len(), 2);
        assert!(matches!(root.left, NodeHandle::Hash(_)));
        assert!(matches!(root.right, NodeHandle::InMemory(_)));
    }

    #[test]
    fn retained_frontier_stays_bounded_across_disjoint_commits() {
        let identifier = vec![];
        let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> = BonsaiStorage::new(
            HashMapDb::<BasicId>::default(),
            BonsaiStorageConfig::default(),
            8,
        );
        let mut id_builder = BasicIdBuilder::new();

        for (key, value) in [
            (vec![0b0000_0000], Felt::from_hex("0x11").unwrap()),
            (vec![0b1000_0000], Felt::from_hex("0x22").unwrap()),
            (vec![0b1100_0000], Felt::from_hex("0x33").unwrap()),
        ] {
            bonsai_storage
                .insert(&identifier, &BitVec::from_vec(key), &value)
                .unwrap();
            bonsai_storage.commit(id_builder.new_id()).unwrap();
        }

        let tree = retained_tree(&bonsai_storage, &identifier);
        assert!(
            tree.nodes.len() <= 4,
            "retained frontier should stay small for a single recently touched branch, got {} nodes",
            tree.nodes.len()
        );
    }

    #[test]
    fn duplicate_staged_insert_invalidates_cached_root() {
        let identifier = vec![];
        let key = BitVec::from_vec(vec![0b1000_0000]);
        let value_one = Felt::from_hex("0x11").unwrap();
        let value_two = Felt::from_hex("0x22").unwrap();

        let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> = BonsaiStorage::new(
            HashMapDb::<BasicId>::default(),
            BonsaiStorageConfig::default(),
            8,
        );
        let mut id_builder = BasicIdBuilder::new();

        bonsai_storage
            .insert(&identifier, &key, &value_one)
            .unwrap();
        let staged_root = bonsai_storage.root_hash_staged(&identifier).unwrap();

        bonsai_storage
            .insert(&identifier, &key, &value_two)
            .unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        let committed_root = bonsai_storage.root_hash(&identifier).unwrap();

        let mut comparison_storage: BonsaiStorage<_, _, Pedersen> = BonsaiStorage::new(
            HashMapDb::<BasicId>::default(),
            BonsaiStorageConfig::default(),
            8,
        );
        let mut comparison_ids = BasicIdBuilder::new();
        comparison_storage
            .insert(&identifier, &key, &value_two)
            .unwrap();
        comparison_storage.commit(comparison_ids.new_id()).unwrap();
        let expected_root = comparison_storage.root_hash(&identifier).unwrap();

        assert_ne!(staged_root, expected_root);
        assert_eq!(committed_root, expected_root);
    }

    #[test]
    fn restoring_committed_value_overrides_staged_mutation() {
        let identifier = vec![];
        let key = BitVec::from_vec(vec![0b1000_0000]);
        let committed_value = Felt::from_hex("0x11").unwrap();
        let transient_value = Felt::from_hex("0x22").unwrap();

        let mut bonsai_storage: BonsaiStorage<_, _, Pedersen> = BonsaiStorage::new(
            HashMapDb::<BasicId>::default(),
            BonsaiStorageConfig::default(),
            8,
        );
        let mut id_builder = BasicIdBuilder::new();

        bonsai_storage
            .insert(&identifier, &key, &committed_value)
            .unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();
        let original_root = bonsai_storage.root_hash(&identifier).unwrap();

        bonsai_storage
            .insert(&identifier, &key, &transient_value)
            .unwrap();
        bonsai_storage
            .insert(&identifier, &key, &committed_value)
            .unwrap();
        bonsai_storage.commit(id_builder.new_id()).unwrap();

        assert_eq!(
            bonsai_storage.root_hash(&identifier).unwrap(),
            original_root
        );
    }
}
