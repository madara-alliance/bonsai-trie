//! Contains constructs for describing the nodes in a Binary Merkle Patricia Tree
//! used by Starknet.
//!
//! For more information about how these Starknet trees are structured, see
//! [`MerkleTree`](super::merkle_tree::MerkleTree).

use crate::BitSlice;
use core::fmt;
use parity_scale_codec::{Decode, Encode};
use starknet_types_core::felt::Felt;

use super::{path::Path, tree::NodeKey};

/// A node in a Binary Merkle-Patricia Tree graph.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub enum Node {
    /// A branch node with exactly two children.
    Binary(BinaryNode),
    /// Describes a path connecting two other nodes.
    Edge(EdgeNode),
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub enum NodeHandle {
    Hash(Felt),
    InMemory(NodeKey),
}
impl NodeHandle {
    pub fn as_hash(self) -> Option<Felt> {
        match self {
            NodeHandle::Hash(felt) => Some(felt),
            NodeHandle::InMemory(_) => None,
        }
    }
}


impl fmt::Debug for NodeHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeHandle::Hash(felt) => write!(f, "Hash({:#x})", felt),
            NodeHandle::InMemory(node_id) => write!(f, "InMemory({:?})", node_id),
        }
    }
}

/// Describes the [Node::Binary] variant.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct BinaryNode {
    /// The hash of this node. Is [None] if the node
    /// has not yet been committed.
    pub hash: Option<Felt>,
    /// The height of this node in the tree.
    // TODO: this field will be removed in the future.
    pub height: u64,
    /// [Left](Direction::Left) child.
    pub left: NodeHandle,
    /// [Right](Direction::Right) child.
    pub right: NodeHandle,
}

/// Node that is an edge.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct EdgeNode {
    /// The hash of this node. Is [None] if the node
    /// has not yet been committed.
    pub hash: Option<Felt>,
    /// The starting height of this node in the tree.
    // TODO: this field will be removed in the future.
    pub height: u64,
    /// The path this edge takes.
    pub path: Path,
    /// The child of this node.
    pub child: NodeHandle,
}

/// Describes the direction a child of a [BinaryNode] may have.
///
/// Binary nodes have two children, one left and one right.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub enum Direction {
    /// Left direction.
    Left,
    /// Right direction.
    Right,
}

impl Direction {
    /// Inverts the [Direction].
    ///
    /// [Left] becomes [Right], and [Right] becomes [Left].
    ///
    /// [Left]: Direction::Left
    /// [Right]: Direction::Right
    pub fn invert(self) -> Direction {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }
}

impl From<bool> for Direction {
    fn from(tf: bool) -> Self {
        match tf {
            true => Direction::Right,
            false => Direction::Left,
        }
    }
}

impl From<Direction> for bool {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::Left => false,
            Direction::Right => true,
        }
    }
}

impl BinaryNode {
    /// Maps the key's bit at the binary node's height to a [Direction].
    ///
    /// This can be used to check which direction the key describes in the context
    /// of this binary node i.e. which direction the child along the key's path would
    /// take.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to get the direction of.
    ///
    /// # Returns
    ///
    /// The direction of the key.
    pub fn direction(&self, key: &BitSlice) -> Direction {
        key[self.height as usize].into()
    }

    /// Returns the [Left] or [Right] child.
    ///
    /// [Left]: Direction::Left
    /// [Right]: Direction::Right
    ///
    /// # Arguments
    ///
    /// `direction` - The direction where to get the child from.
    ///
    /// # Returns
    ///
    /// The child in the specified direction.
    pub fn get_child(&self, direction: Direction) -> NodeHandle {
        match direction {
            Direction::Left => self.left,
            Direction::Right => self.right,
        }
    }

    /// Returns the [Left] or [Right] child.
    ///
    /// [Left]: Direction::Left
    /// [Right]: Direction::Right
    ///
    /// # Arguments
    ///
    /// `direction` - The direction where to get the child from.
    ///
    /// # Returns
    ///
    /// The child in the specified direction.
    pub fn get_child_mut(&mut self, direction: Direction) -> &mut NodeHandle {
        match direction {
            Direction::Left => &mut self.left,
            Direction::Right => &mut self.right,
        }
    }
}

impl Node {
    /// Convert to node to binary node type (returns None if it's not a binary node).
    pub fn as_binary(&self) -> Option<&BinaryNode> {
        match self {
            Node::Binary(node) => Some(node),
            _ => None,
        }
    }
    /// Convert to node to binary node type (returns None if it's not a binary node).
    pub fn as_binary_mut(&mut self) -> Option<&mut BinaryNode> {
        match self {
            Node::Binary(node) => Some(node),
            _ => None,
        }
    }
    /// Convert to node to edge node type (returns None if it's not an edge node).
    pub fn as_edge(&self) -> Option<&EdgeNode> {
        match self {
            Node::Edge(node) => Some(node),
            _ => None,
        }
    }
    /// Convert to node to edge node type (returns None if it's not an edge node).
    pub fn as_edge_mut(&mut self) -> Option<&mut EdgeNode> {
        match self {
            Node::Edge(node) => Some(node),
            _ => None,
        }
    }

    pub fn get_hash(&self) -> Option<Felt> {
        match self {
            Node::Binary(binary) => binary.hash,
            Node::Edge(edge) => edge.hash,
        }
    }
}

impl EdgeNode {
    /// Returns true if the edge node's path matches the same path given by the key.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to check if the path matches with the edge node.
    /// * `node_height` - The height of the edge node.
    pub fn path_matches(&self, key: &BitSlice, node_height: usize) -> bool {
        assert_eq!(self.height as usize, node_height);
        let lower_bound = node_height.min(key.len());
        let upper_bound = (node_height + self.path.0.len()).min(key.len());
        log::trace!(
            "path_matches {:b}{lower_bound}..{upper_bound} ({}) - {:b}0..{}",
            &key[lower_bound..upper_bound],
            upper_bound - lower_bound,
            self.path.0,
            self.path.len()
        );
        self.path.starts_with(&key[lower_bound..upper_bound])
    }

    /// Returns the common bit prefix between the edge node's path and the given key.
    ///
    /// This is calculated with the edge's height taken into account.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to get the common path from.
    pub fn common_path(&self, key: &BitSlice) -> &BitSlice {
        let key_path = key.iter().skip(self.height as usize);
        let common_length = key_path
            .zip(self.path.0.iter())
            .take_while(|(a, b)| a == b)
            .count();

        &self.path.0[..common_length]
    }
}

#[test]
fn test_path_matches_basic() {
    let path =
        Path(BitSlice::from_slice(&[0b10101010, 0b01010101, 0b10101010, 0b01010101]).to_bitvec());
    let edge = EdgeNode {
        hash: None,
        height: 0,
        path,
        child: NodeHandle::Hash(Felt::ZERO),
    };

    let key = BitSlice::from_slice(&[0b10101010, 0b01010101, 0b10101010, 0b01010101]);
    assert!(edge.path_matches(key, 0));
}

#[test]
fn test_path_matches_with_height() {
    let path =
        Path(BitSlice::from_slice(&[0b10101010, 0b01010101, 0b10101010, 0b01010101]).to_bitvec());
    let edge = EdgeNode {
        hash: None,
        height: 8,
        path,
        child: NodeHandle::Hash(Felt::ZERO),
    };

    let key = BitSlice::from_slice(&[0b10101010, 0b10101010, 0b01010101, 0b10101010, 0b01010101]);
    assert!(edge.path_matches(key, 8));
}

#[test]
fn test_path_matches_only_part_with_height() {
    let path =
        Path(BitSlice::from_slice(&[0b10101010, 0b01010101, 0b10101010, 0b01010101]).to_bitvec());
    let edge = EdgeNode {
        hash: None,
        height: 8,
        path,
        child: NodeHandle::Hash(Felt::ZERO),
    };

    let key = BitSlice::from_slice(&[
        0b10101010, 0b10101010, 0b01010101, 0b10101010, 0b01010101, 0b10101010,
    ]);
    assert!(edge.path_matches(key, 8));
}

#[test]
fn test_path_dont_match() {
    let path =
        Path(BitSlice::from_slice(&[0b10111010, 0b01010101, 0b10101010, 0b01010101]).to_bitvec());
    let edge = EdgeNode {
        hash: None,
        height: 0,
        path,
        child: NodeHandle::Hash(Felt::ZERO),
    };

    let key = BitSlice::from_slice(&[0b10101010, 0b01010101, 0b10101010, 0b01010101, 0b10101010]);
    assert!(!edge.path_matches(key, 0));
}

#[test]
fn test_common_path_basic() {
    let path =
        Path(BitSlice::from_slice(&[0b10101010, 0b01010101, 0b10101010, 0b01010101]).to_bitvec());
    let edge = EdgeNode {
        hash: None,
        height: 0,
        path: path.clone(),
        child: NodeHandle::Hash(Felt::ZERO),
    };

    let key = BitSlice::from_slice(&[0b10101010, 0b01010101, 0b10101010, 0b01010101]);
    assert_eq!(edge.common_path(key), &path.0);
}

#[test]
fn test_common_path_only_part() {
    let path =
        Path(BitSlice::from_slice(&[0b10101010, 0b01010101, 0b10101010, 0b01010101]).to_bitvec());
    let edge = EdgeNode {
        hash: None,
        height: 0,
        path,
        child: NodeHandle::Hash(Felt::ZERO),
    };

    let key = BitSlice::from_slice(&[0b10101010, 0b01010101]);
    assert_eq!(
        edge.common_path(key),
        BitSlice::from_slice(&[0b10101010, 0b01010101])
    );
}

#[test]
fn test_common_path_part_with_height() {
    let path =
        Path(BitSlice::from_slice(&[0b10101010, 0b01010101, 0b10101010, 0b01010101]).to_bitvec());
    let edge = EdgeNode {
        hash: None,
        height: 8,
        path,
        child: NodeHandle::Hash(Felt::ZERO),
    };

    let key = BitSlice::from_slice(&[0b01010101, 0b10101010]);
    assert_eq!(edge.common_path(key), BitSlice::from_slice(&[0b10101010]));
}

#[test]
fn test_no_common_path() {
    let path =
        Path(BitSlice::from_slice(&[0b10101010, 0b01010101, 0b10101010, 0b01010101]).to_bitvec());
    let edge = EdgeNode {
        hash: None,
        height: 0,
        path,
        child: NodeHandle::Hash(Felt::ZERO),
    };

    let key = BitSlice::from_slice(&[0b01010101, 0b10101010]);
    assert_eq!(edge.common_path(key), BitSlice::empty());
}
