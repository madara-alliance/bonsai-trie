use crate::{id::Id, ByteVec, Vec};
#[cfg(feature = "std")]
use std::error::Error;

/// Key in the database of the different elements that can be stored in the database.
#[derive(Debug, Hash, PartialEq, Eq)]
pub enum DatabaseKey<'a> {
    Trie(&'a [u8]),
    Flat(&'a [u8]),
    TrieLog(&'a [u8]),
}

impl DatabaseKey<'_> {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            DatabaseKey::Trie(slice) => slice,
            DatabaseKey::Flat(slice) => slice,
            DatabaseKey::TrieLog(slice) => slice,
        }
    }
}

#[cfg(feature = "std")]
pub trait DBError: Error + Send + Sync {}

#[cfg(not(feature = "std"))]
pub trait DBError: Send + Sync {}

/// Trait to be implemented on any type that can be used as a database.
pub trait BonsaiDatabase: core::fmt::Debug {
    type Batch: Default;
    #[cfg(feature = "std")]
    type DatabaseError: Error + DBError;
    #[cfg(not(feature = "std"))]
    type DatabaseError: DBError;

    /// Create a new empty batch of changes to be used in `insert`, `remove` and applied in database using `write_batch`.
    fn create_batch(&self) -> Self::Batch;

    /// Returns the value of the key if it exists
    fn get(&self, key: &DatabaseKey) -> Result<Option<ByteVec>, Self::DatabaseError>;

    #[allow(clippy::type_complexity)]
    /// Returns all values with keys that start with the given prefix
    fn get_by_prefix(
        &self,
        prefix: &DatabaseKey,
    ) -> Result<Vec<(ByteVec, ByteVec)>, Self::DatabaseError>;

    /// Returns true if the key exists
    fn contains(&self, key: &DatabaseKey) -> Result<bool, Self::DatabaseError>;

    /// Insert a new key-value pair, returns the old value if it existed.
    /// If a batch is provided, the change will be written in the batch instead of the database.
    fn insert(
        &mut self,
        key: &DatabaseKey,
        value: &[u8],
        batch: Option<&mut Self::Batch>,
    ) -> Result<Option<ByteVec>, Self::DatabaseError>;

    /// Remove a key-value pair, returns the old value if it existed.
    /// If a batch is provided, the change will be written in the batch instead of the database.
    fn remove(
        &mut self,
        key: &DatabaseKey,
        batch: Option<&mut Self::Batch>,
    ) -> Result<Option<ByteVec>, Self::DatabaseError>;

    /// Remove all keys that start with the given prefix
    fn remove_by_prefix(&mut self, prefix: &DatabaseKey) -> Result<(), Self::DatabaseError>;

    /// Write batch of changes directly in the database
    fn write_batch(&mut self, batch: Self::Batch) -> Result<(), Self::DatabaseError>;

    /// Functions available in tests to display the whole database key/values
    #[cfg(test)]
    fn dump_database(&self);
}

pub trait BonsaiPersistentDatabase<ID: Id> {
    type Transaction<'a>: BonsaiDatabase<DatabaseError = Self::DatabaseError>
    where
        Self: 'a;
    #[cfg(feature = "std")]
    type DatabaseError: Error + DBError;
    #[cfg(not(feature = "std"))]
    type DatabaseError: DBError;
    /// Save a snapshot of the current database state
    /// This function returns a snapshot id that can be used to create a transaction
    fn snapshot(&mut self, id: ID);

    /// Create a transaction based on the given snapshot id
    fn transaction(&self, id: ID) -> Option<(ID, Self::Transaction<'_>)>;

    /// Merge a transaction in the current persistent database
    fn merge<'a>(&mut self, transaction: Self::Transaction<'a>) -> Result<(), Self::DatabaseError>
    where
        Self: 'a;
}
