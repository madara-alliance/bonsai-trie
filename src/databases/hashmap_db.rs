use crate::{
    bonsai_database::{BonsaiPersistentDatabase, DBError},
    id::Id,
    BTreeMap, BonsaiDatabase, HashMap, Vec,
};
use crate::{ByteVec, DatabaseKey};
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

#[derive(Clone, Default, Debug)]
pub struct HashMapDb<ID: Id> {
    trie_db: HashMap<ByteVec, ByteVec>,
    flat_db: HashMap<ByteVec, ByteVec>,
    trie_log_db: HashMap<ByteVec, ByteVec>,
    snapshots: BTreeMap<ID, HashMapDb<ID>>,
}

impl<ID: Id> HashMapDb<ID> {
    fn get_map(&self, key: &DatabaseKey) -> &HashMap<ByteVec, ByteVec> {
        match key {
            DatabaseKey::Trie(_) => &self.trie_db,
            DatabaseKey::Flat(_) => &self.flat_db,
            DatabaseKey::TrieLog(_) => &self.trie_log_db,
        }
    }
    fn get_map_mut(&mut self, key: &DatabaseKey) -> &mut HashMap<ByteVec, ByteVec> {
        match key {
            DatabaseKey::Trie(_) => &mut self.trie_db,
            DatabaseKey::Flat(_) => &mut self.flat_db,
            DatabaseKey::TrieLog(_) => &mut self.trie_log_db,
        }
    }

    #[cfg(test)]
    pub(crate) fn assert_empty(&self) {
        assert_eq!(self.trie_db, [].into());
        assert_eq!(self.flat_db, [].into());
    }
}

impl<ID: Id> BonsaiDatabase for HashMapDb<ID> {
    type Batch = ();
    type DatabaseError = HashMapDbError;

    fn create_batch(&self) -> Self::Batch {}

    fn remove_by_prefix(&mut self, prefix: &DatabaseKey) -> Result<(), Self::DatabaseError> {
        let mut keys_to_remove = Vec::new();
        let db = self.get_map_mut(prefix);
        for key in db.keys() {
            if key.starts_with(prefix.as_slice()) {
                keys_to_remove.push(key.clone());
            }
        }
        for key in keys_to_remove {
            db.remove(&key);
        }
        Ok(())
    }

    fn get(&self, key: &DatabaseKey) -> Result<Option<ByteVec>, Self::DatabaseError> {
        let db = &self.get_map(key);
        Ok(db.get(key.as_slice()).cloned())
    }

    fn get_by_prefix(
        &self,
        prefix: &DatabaseKey,
    ) -> Result<Vec<(ByteVec, ByteVec)>, Self::DatabaseError> {
        let mut result = Vec::new();
        let db = self.get_map(prefix);
        for (key, value) in db.iter() {
            if key.starts_with(prefix.as_slice()) {
                result.push((key.clone(), value.clone()));
            }
        }
        Ok(result)
    }

    fn insert(
        &mut self,
        key: &DatabaseKey,
        value: &[u8],
        _batch: Option<&mut Self::Batch>,
    ) -> Result<Option<ByteVec>, Self::DatabaseError> {
        let db = self.get_map_mut(key);
        Ok(db.insert(key.as_slice().into(), value.into()))
    }

    fn remove(
        &mut self,
        key: &DatabaseKey,
        _batch: Option<&mut Self::Batch>,
    ) -> Result<Option<ByteVec>, Self::DatabaseError> {
        let db = self.get_map_mut(key);
        Ok(db.remove(key.as_slice()))
    }

    fn contains(&self, key: &DatabaseKey) -> Result<bool, Self::DatabaseError> {
        let db = self.get_map(key);
        Ok(db.contains_key(key.as_slice()))
    }

    fn write_batch(&mut self, _batch: Self::Batch) -> Result<(), Self::DatabaseError> {
        Ok(())
    }

    #[cfg(test)]
    fn dump_database(&self) {
        log::debug!("{:?}", self);
    }
}

impl<ID: Id> BonsaiPersistentDatabase<ID> for HashMapDb<ID> {
    type DatabaseError = HashMapDbError;
    type Transaction<'a>
        = HashMapDb<ID>
    where
        ID: 'a;
    fn snapshot(&mut self, id: ID) {
        self.snapshots.insert(id, self.clone());
    }

    fn transaction(&self, id: ID) -> Option<(ID, Self::Transaction<'_>)> {
        self.snapshots
            .range(..&id)
            .next()
            .map(|(id, snapshot)| (*id, snapshot.clone()))
    }

    fn merge<'a>(&mut self, transaction: Self::Transaction<'a>) -> Result<(), Self::DatabaseError>
    where
        ID: 'a,
    {
        self.trie_db = transaction.trie_db;
        self.flat_db = transaction.flat_db;
        self.trie_log_db = transaction.trie_log_db;
        Ok(())
    }
}
