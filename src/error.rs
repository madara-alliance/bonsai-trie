#[cfg(feature = "std")]
use std::{error::Error, fmt::Display};

use crate::{bonsai_database::DBError, String};

/// All errors that can be returned by BonsaiStorage.
#[derive(Debug)]
pub enum BonsaiStorageError<DatabaseError>
where
    DatabaseError: DBError,
{
    /// Error from the underlying trie.
    Trie(String),
    /// Error when trying to go to a specific commit ID.
    GoTo(String),
    /// Error when working with a transactional state.
    Transaction(String),
    /// Error when trying to merge a transactional state.
    Merge(String),
    /// Error from the underlying database.
    Database(DatabaseError),
    /// Error when decoding a node
    NodeDecodeError(parity_scale_codec::Error),
}

impl<DatabaseError: DBError> core::convert::From<DatabaseError>
    for BonsaiStorageError<DatabaseError>
{
    fn from(value: DatabaseError) -> Self {
        Self::Database(value)
    }
}

impl<DatabaseError: DBError> core::convert::From<parity_scale_codec::Error>
    for BonsaiStorageError<DatabaseError>
{
    fn from(value: parity_scale_codec::Error) -> Self {
        Self::NodeDecodeError(value)
    }
}

#[cfg(feature = "std")]
impl<DatabaseError> Display for BonsaiStorageError<DatabaseError>
where
    DatabaseError: Error + DBError,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BonsaiStorageError::Trie(e) => write!(f, "Trie error: {}", e),
            BonsaiStorageError::GoTo(e) => write!(f, "GoTo error: {}", e),
            BonsaiStorageError::Transaction(e) => write!(f, "Transaction error: {}", e),
            BonsaiStorageError::Merge(e) => write!(f, "Merge error: {}", e),
            BonsaiStorageError::Database(e) => write!(f, "Database error: {}", e),
            BonsaiStorageError::NodeDecodeError(e) => write!(f, "Node decode error: {}", e),
        }
    }
}
