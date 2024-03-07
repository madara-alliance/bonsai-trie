#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::{fmt::Debug, hash};

/// Trait to be implemented on any type that can be used as an ID.
pub trait Id: hash::Hash + PartialEq + Eq + PartialOrd + Ord + Debug + Copy + Default {
    fn to_bytes(&self) -> Vec<u8>;
}

/// A basic ID type that can be used for testing.
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Default)]
pub struct BasicId(u64);

impl BasicId {
    /// Constructor for creating a new `BasicId` from a `u64`.
    pub fn new(id: u64) -> Self {
        BasicId(id)
    }

    /// Converts the ID to a byte vector.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_be_bytes().to_vec()
    }
}

impl Id for BasicId {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_bytes()
    }
}

impl From<u64> for BasicId {
    fn from(item: u64) -> Self {
        BasicId::new(item)
    }
}

/// A builder for basic IDs.
pub struct BasicIdBuilder {
    last_id: u64,
}

impl Default for BasicIdBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl BasicIdBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self { last_id: 0 }
    }

    /// Create a new ID (unique).
    pub fn new_id(&mut self) -> BasicId {
        let id = BasicId(self.last_id);
        self.last_id = self.last_id.checked_add(1).expect("Id overflow");
        id
    }
}
