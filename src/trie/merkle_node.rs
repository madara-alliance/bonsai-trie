//! Contains constructs for describing the nodes in a Binary Merkle Patricia Tree
//! used by Starknet.
//!
//! For more information about how these Starknet trees are structured, see
//! [`MerkleTree`](super::merkle_tree::MerkleTree).

use bitvec::order::Msb0;
use bitvec::slice::BitSlice;
use mp_felt::Felt252Wrapper;
use parity_scale_codec::{Decode, Encode};

use super::merkle_tree::Path;

/// Id of a Node within the tree
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct NodeId(pub u64);

impl NodeId {
    /// Mutates the given NodeId to be the next one and returns it.
    pub fn next_id(&mut self) -> NodeId {
        self.0 += 1;
        NodeId(self.0)
    }

    pub fn reset(&mut self) {
        self.0 = 0;
    }
}

/// A node in a Binary Merkle-Patricia Tree graph.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub enum Node {
    /// A node that has not been fetched from storage yet.
    ///
    /// As such, all we know is its hash.
    Unresolved(Felt252Wrapper),
    /// A branch node with exactly two children.
    Binary(BinaryNode),
    /// Describes a path connecting two other nodes.
    Edge(EdgeNode),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub enum NodeHandle {
    Hash(Felt252Wrapper),
    InMemory(NodeId),
}

/// Describes the [Node::Binary] variant.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct BinaryNode {
    /// The hash of this node.
    pub hash: Option<Felt252Wrapper>,
    /// The height of this node in the tree.
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
    pub hash: Option<Felt252Wrapper>,
    /// The starting height of this node in the tree.
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
    pub fn direction(&self, key: &BitSlice<u8, Msb0>) -> Direction {
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
}

impl Node {
    /// Returns true if the node represents an empty node -- this is defined as a node
    /// with the [Felt252Wrapper::ZERO].
    ///
    /// This can occur for the root node in an empty graph.
    pub fn is_empty(&self) -> bool {
        match self {
            Node::Unresolved(hash) => hash == &Felt252Wrapper::ZERO,
            _ => false,
        }
    }

    /// Is the node a binary node.
    pub fn is_binary(&self) -> bool {
        matches!(self, Node::Binary(..))
    }

    /// Convert to node to binary node type (returns None if it's not a binary node).
    pub fn as_binary(&self) -> Option<&BinaryNode> {
        match self {
            Node::Binary(binary) => Some(binary),
            _ => None,
        }
    }

    pub fn hash(&self) -> Option<Felt252Wrapper> {
        match self {
            Node::Unresolved(hash) => Some(*hash),
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
    pub fn path_matches(&self, key: &BitSlice<u8, Msb0>) -> bool {
        self.path.0
            == key[(self.height as usize)..(self.height + self.path.0.len() as u64) as usize]
    }

    /// Returns the common bit prefix between the edge node's path and the given key.
    ///
    /// This is calculated with the edge's height taken into account.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to get the common path from.
    pub fn common_path(&self, key: &BitSlice<u8, Msb0>) -> &BitSlice<u8, Msb0> {
        let key_path = key.iter().skip(self.height as usize);
        let common_length = key_path
            .zip(self.path.0.iter())
            .take_while(|(a, b)| a == b)
            .count();

        &self.path.0[..common_length]
    }
}
