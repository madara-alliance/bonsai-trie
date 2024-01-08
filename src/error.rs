use std::{error::Error, fmt::Display};

use mp_felt::Felt252WrapperError;

use crate::bonsai_database::DBError;

/// All errors that can be returned by BonsaiStorage.
#[derive(Debug, thiserror::Error)]
pub enum BonsaiStorageError<DatabaseError>
where
    DatabaseError: Error + DBError,
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
    Database(#[from] DatabaseError),
    /// Error from Felt conversion
    Felt252WrapperError(#[from] Felt252WrapperError),
    /// Error when decoding a node
    NodeDecodeError(#[from] parity_scale_codec::Error),
}

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
            BonsaiStorageError::Felt252WrapperError(e) => write!(f, "Felt error: {}", e),
            BonsaiStorageError::NodeDecodeError(e) => write!(f, "Node decode error: {}", e),
        }
    }
}
