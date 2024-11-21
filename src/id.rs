use crate::ByteVec;
use core::{fmt::Debug, hash};

/// Trait to be implemented on any type that can be used as an ID.
pub trait Id: hash::Hash + PartialEq + Eq + PartialOrd + Ord + Debug + Copy + Default {
    fn to_bytes(&self) -> ByteVec;
    fn as_u64(self) -> u64;
    fn from_u64(v: u64) -> Self;
}

/// A basic ID type that can be used for testing.
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Default)]
pub struct BasicId(u64);

impl BasicId {
    pub fn new(id: u64) -> Self {
        BasicId(id)
    }
}

impl Id for BasicId {
    fn to_bytes(&self) -> ByteVec {
        ByteVec::from(&self.0.to_be_bytes() as &[_])
    }
    fn as_u64(self) -> u64 {
        self.0
    }
    fn from_u64(v: u64) -> Self {
        Self(v)
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
