use alloc::fmt::Display;
use alloc::vec::Vec;
use alloc::{collections::BTreeMap, string::ToString};
#[cfg(not(feature = "std"))]
use hashbrown::HashMap;
#[cfg(feature = "std")]
use std::collections::HashMap;

use crate::{
    bonsai_database::BonsaiPersistentDatabase, error::BonsaiStorageError, id::Id, BonsaiDatabase,
};

#[derive(Debug)]
pub struct HashMapDbError {}

impl Display for HashMapDbError {
    fn fmt(&self, f: &mut alloc::fmt::Formatter<'_>) -> alloc::fmt::Result {
        write!(f, "")
    }
}

impl From<HashMapDbError> for BonsaiStorageError {
    fn from(err: HashMapDbError) -> Self {
        Self::Database(err.to_string())
    }
}

#[derive(Clone, Default)]
pub struct HashMapDbConfig {}

#[derive(Clone)]
pub struct HashMapDb<ID: Id> {
    config: HashMapDbConfig,
    db: HashMap<Vec<u8>, Vec<u8>>,
    snapshots: BTreeMap<ID, HashMapDb<ID>>,
}

impl<ID: Id> HashMapDb<ID> {
    pub fn new(config: HashMapDbConfig) -> Self {
        Self {
            config,
            db: HashMap::new(),
            snapshots: BTreeMap::new(),
        }
    }
}

impl<ID: Id> BonsaiDatabase for HashMapDb<ID> {
    type Batch = ();
    type DatabaseError = HashMapDbError;

    fn create_batch(&self) -> Self::Batch {}

    fn remove_by_prefix(
        &mut self,
        prefix: &crate::bonsai_database::KeyType,
    ) -> Result<(), Self::DatabaseError> {
        let mut keys_to_remove = Vec::new();
        for key in self.db.keys() {
            if key.starts_with(prefix.as_slice()) {
                keys_to_remove.push(key.clone());
            }
        }
        for key in keys_to_remove {
            self.db.remove(&key);
        }
        Ok(())
    }

    fn get(
        &self,
        key: &crate::bonsai_database::KeyType,
    ) -> Result<Option<Vec<u8>>, Self::DatabaseError> {
        Ok(self.db.get(key.as_slice()).cloned())
    }

    fn get_by_prefix(
        &self,
        prefix: &crate::bonsai_database::KeyType,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, Self::DatabaseError> {
        let mut result = Vec::new();
        for (key, value) in self.db.iter() {
            if key.starts_with(prefix.as_slice()) {
                result.push((key.clone(), value.clone()));
            }
        }
        Ok(result)
    }

    fn insert(
        &mut self,
        key: &crate::bonsai_database::KeyType,
        value: &[u8],
        _batch: Option<&mut Self::Batch>,
    ) -> Result<Option<Vec<u8>>, Self::DatabaseError> {
        Ok(self.db.insert(key.as_slice().to_vec(), value.to_vec()))
    }

    fn remove(
        &mut self,
        key: &crate::bonsai_database::KeyType,
        _batch: Option<&mut Self::Batch>,
    ) -> Result<Option<Vec<u8>>, Self::DatabaseError> {
        Ok(self.db.remove(key.as_slice()))
    }

    fn contains(&self, key: &crate::bonsai_database::KeyType) -> Result<bool, Self::DatabaseError> {
        Ok(self.db.contains_key(key.as_slice()))
    }

    fn write_batch(&mut self, _batch: Self::Batch) -> Result<(), Self::DatabaseError> {
        Ok(())
    }

    #[cfg(test)]
    fn dump_database(&self) {
        println!("{:?}", self.db);
    }
}

impl<ID: Id> BonsaiPersistentDatabase<ID> for HashMapDb<ID> {
    type DatabaseError = HashMapDbError;
    type Transaction = HashMapDb<ID>;
    fn snapshot(&mut self, id: ID) {
        self.snapshots.insert(id, self.clone());
    }

    fn transaction(&self, id: ID) -> Option<Self::Transaction> {
        self.snapshots.get(&id).cloned()
    }

    fn merge(&mut self, transaction: Self::Transaction) -> Result<(), Self::DatabaseError> {
        self.db = transaction.db;
        Ok(())
    }
}
