#[cfg(not(feature = "std"))]
use alloc::string::String;
/// All errors that can be returned by BonsaiStorage.
#[derive(Debug)]
pub enum BonsaiStorageError {
    /// Error from the underlying trie.
    Trie(String),
    /// Error when trying to go to a specific commit ID.
    GoTo(String),
    /// Error when working with a transactional state.
    Transaction(String),
    /// Error when trying to merge a transactional state.
    Merge(String),
    /// Error from the underlying database.
    Database(String),
}
