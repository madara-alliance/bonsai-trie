#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::{fmt::Debug, hash};

/// Trait to be implemented on any type that can be used as an ID.
pub trait Id: hash::Hash + PartialEq + Eq + PartialOrd + Ord + Debug + Copy {
    fn serialize(&self) -> Vec<u8>;
}

/// A basic ID type that can be used for testing.
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct BasicId(u64);

impl Id for BasicId {
    fn serialize(&self) -> Vec<u8> {
        self.0.to_be_bytes().to_vec()
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
        self.last_id += 1;
        BasicId(self.last_id)
    }
}
