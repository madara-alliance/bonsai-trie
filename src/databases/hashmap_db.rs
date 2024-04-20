use crate::SByteVec;
use crate::{
    bonsai_database::{BonsaiPersistentDatabase, DBError},
    id::Id,
    BTreeMap, BonsaiDatabase, HashMap, Vec,
};
use core::{fmt, fmt::Display};

#[derive(Debug)]
pub struct HashMapDbError {}

#[cfg(feature = "std")]
impl std::error::Error for HashMapDbError {}

impl Display for HashMapDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "")
    }
}

impl DBError for HashMapDbError {}

#[derive(Clone, Default)]
pub struct HashMapDb<ID: Id> {
    db: HashMap<SByteVec, SByteVec>,
    snapshots: BTreeMap<ID, HashMapDb<ID>>,
}

impl<ID: Id> BonsaiDatabase for HashMapDb<ID> {
    type Batch = ();
    type DatabaseError = HashMapDbError;

    fn create_batch(&self) -> Self::Batch {}

    fn remove_by_prefix(
        &mut self,
        prefix: &crate::bonsai_database::DatabaseKey,
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
        key: &crate::bonsai_database::DatabaseKey,
    ) -> Result<Option<SByteVec>, Self::DatabaseError> {
        Ok(self.db.get(key.as_slice()).cloned())
    }

    fn get_by_prefix(
        &self,
        prefix: &crate::bonsai_database::DatabaseKey,
    ) -> Result<Vec<(SByteVec, SByteVec)>, Self::DatabaseError> {
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
        key: &crate::bonsai_database::DatabaseKey,
        value: &[u8],
        _batch: Option<&mut Self::Batch>,
    ) -> Result<Option<SByteVec>, Self::DatabaseError> {
        Ok(self.db.insert(key.as_slice().into(), value.into()))
    }

    fn remove(
        &mut self,
        key: &crate::bonsai_database::DatabaseKey,
        _batch: Option<&mut Self::Batch>,
    ) -> Result<Option<SByteVec>, Self::DatabaseError> {
        Ok(self.db.remove(key.as_slice()))
    }

    fn contains(
        &self,
        key: &crate::bonsai_database::DatabaseKey,
    ) -> Result<bool, Self::DatabaseError> {
        Ok(self.db.contains_key(key.as_slice()))
    }

    fn write_batch(&mut self, _batch: Self::Batch) -> Result<(), Self::DatabaseError> {
        Ok(())
    }

    #[cfg(test)]
    fn dump_database(&self) {
        log::debug!("{:?}", self.db);
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
