use std::{
    collections::{BTreeMap, HashMap},
    error::Error as StdError,
    fmt,
    path::Path,
};

use rocksdb::{
    ColumnFamilyDescriptor, ColumnFamilyRef, Direction, Error, IteratorMode, MultiThreaded,
    OptimisticTransactionDB, OptimisticTransactionOptions, Options, ReadOptions,
    SnapshotWithThreadMode, Transaction, WriteBatchWithTransaction, WriteOptions,
};

use crate::{
    bonsai_database::{BonsaiDatabase, BonsaiPersistentDatabase, DBError, DatabaseKey},
    id::Id,
    ByteVec,
};
use log::trace;

const TRIE_LOG_CF: &str = "trie_log";
const TRIE_CF: &str = "trie";
const FLAT_CF: &str = "flat";

const CF_ERROR: &str = "critical: rocksdb column family operation failed";

/// Creates a new RocksDB database from the given path
pub fn create_rocks_db(path: impl AsRef<Path>) -> Result<OptimisticTransactionDB, Error> {
    // Delete folder content
    if path.as_ref().exists() {
        std::fs::remove_dir_all(path.as_ref()).unwrap();
    }
    std::fs::create_dir_all(path.as_ref()).unwrap();
    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    let db = OptimisticTransactionDB::<MultiThreaded>::open_cf_descriptors(
        &opts,
        path,
        vec![
            ColumnFamilyDescriptor::new(TRIE_LOG_CF, Options::default()),
            ColumnFamilyDescriptor::new(TRIE_CF, Options::default()),
            ColumnFamilyDescriptor::new(FLAT_CF, Options::default()),
        ],
    )?;

    Ok(db)
}

/// A struct that implements the `BonsaiDatabase` trait using RocksDB as the underlying database
pub struct RocksDB<'db, ID: Id> {
    db: &'db OptimisticTransactionDB<MultiThreaded>,
    config: RocksDBConfig,
    snapshots: BTreeMap<ID, SnapshotWithThreadMode<'db, OptimisticTransactionDB>>,
}

impl<'db, ID: Id> fmt::Debug for RocksDB<'db, ID> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ROCKSDB_DATABASE_DUMP {{")?;
        let handle_trie = self.db.cf_handle(TRIE_CF).expect(CF_ERROR);
        let handle_flat = self.db.cf_handle(FLAT_CF).expect(CF_ERROR);
        let handle_trie_log = self.db.cf_handle(TRIE_LOG_CF).expect(CF_ERROR);
        let mut iter = self.db.raw_iterator_cf(&handle_trie);
        iter.seek_to_first();
        while iter.valid() {
            let key = iter.key().unwrap();
            let value = iter.value().unwrap();
            writeln!(f, "{:?} => {:?},", key, value)?;
            iter.next();
        }
        let mut iter = self.db.raw_iterator_cf(&handle_flat);
        iter.seek_to_first();
        while iter.valid() {
            let key = iter.key().unwrap();
            let value = iter.value().unwrap();
            writeln!(f, "{:?} => {:?},", key, value)?;
            iter.next();
        }
        let mut iter = self.db.raw_iterator_cf(&handle_trie_log);
        iter.seek_to_first();
        while iter.valid() {
            let key = iter.key().unwrap();
            let value = iter.value().unwrap();
            writeln!(f, "{:?} => {:?},", key, value)?;
            iter.next();
        }
        write!(f, "}}")?;
        Ok(())
    }
}

/// Configuration for RocksDB database
pub struct RocksDBConfig {
    /// Maximum number of snapshots kept in database
    pub max_saved_snapshots: Option<usize>,
}

impl Default for RocksDBConfig {
    fn default() -> Self {
        Self {
            max_saved_snapshots: Some(100),
        }
    }
}

impl<'db, ID: Id> RocksDB<'db, ID> {
    /// Creates a new RocksDB wrapper from the given RocksDB database
    pub fn new(db: &'db OptimisticTransactionDB, config: RocksDBConfig) -> Self {
        trace!("RockDB database opened");
        Self {
            db,
            config,
            snapshots: BTreeMap::default(),
        }
    }
}

/// A batch used to write changes in the RocksDB database
pub type RocksDBBatch = WriteBatchWithTransaction<true>;

#[derive(Debug)]
pub enum RocksDBError {
    RocksDB(Error),
    Custom(String),
}

impl From<Error> for RocksDBError {
    fn from(err: Error) -> Self {
        Self::RocksDB(err)
    }
}

impl fmt::Display for RocksDBError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RocksDB(err) => write!(f, "RocksDB error: {}", err),
            Self::Custom(err) => write!(f, "RocksDB error in trie: {}", err),
        }
    }
}

impl DBError for RocksDBError {}

impl StdError for RocksDBError {
    fn cause(&self) -> Option<&dyn StdError> {
        match self {
            Self::RocksDB(err) => Some(err),
            Self::Custom(_) => None,
        }
    }

    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::RocksDB(err) => Some(err),
            Self::Custom(_) => None,
        }
    }
}

impl DatabaseKey<'_> {
    fn get_cf(&self) -> &'static str {
        match self {
            DatabaseKey::Trie(_) => TRIE_CF,
            DatabaseKey::Flat(_) => FLAT_CF,
            DatabaseKey::TrieLog(_) => TRIE_LOG_CF,
        }
    }
}

pub struct RocksDBTransaction<'a> {
    txn: Transaction<'a, OptimisticTransactionDB>,
    read_options: ReadOptions,
    column_families: HashMap<String, ColumnFamilyRef<'a>>,
}

impl<'a> fmt::Debug for RocksDBTransaction<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RocksDBTransaction").finish()
    }
}

impl<'db, ID> BonsaiDatabase for RocksDB<'db, ID>
where
    ID: Id,
{
    type Batch = RocksDBBatch;
    type DatabaseError = RocksDBError;

    fn create_batch(&self) -> Self::Batch {
        Self::Batch::default()
    }

    #[cfg(test)]
    fn dump_database(&self) {
        println!("{:?}", self)
    }

    fn insert(
        &mut self,
        key: &DatabaseKey,
        value: &[u8],
        batch: Option<&mut Self::Batch>,
    ) -> Result<Option<ByteVec>, Self::DatabaseError> {
        trace!("Inserting into RocksDB: {:?} {:?}", key, value);
        let handle_cf = self.db.cf_handle(key.get_cf()).expect(CF_ERROR);
        let old_value = self.db.get_cf(&handle_cf, key.as_slice())?;
        if let Some(batch) = batch {
            batch.put_cf(&handle_cf, key.as_slice(), value);
        } else {
            self.db.put_cf(&handle_cf, key.as_slice(), value)?;
        }
        Ok(old_value.map(Into::into))
    }

    fn get(&self, key: &DatabaseKey) -> Result<Option<ByteVec>, Self::DatabaseError> {
        trace!("Getting from RocksDB: {:?}", key);
        let handle = self.db.cf_handle(key.get_cf()).expect(CF_ERROR);
        Ok(self.db.get_cf(&handle, key.as_slice())?.map(Into::into))
    }

    fn get_by_prefix(
        &self,
        prefix: &DatabaseKey,
    ) -> Result<Vec<(ByteVec, ByteVec)>, Self::DatabaseError> {
        trace!("Getting from RocksDB: {:?}", prefix);
        let handle = self.db.cf_handle(prefix.get_cf()).expect(CF_ERROR);
        let iter = self.db.iterator_cf(
            &handle,
            IteratorMode::From(prefix.as_slice(), Direction::Forward),
        );
        Ok(iter
            .map_while(|kv| {
                if let Ok((key, value)) = kv {
                    if key.starts_with(prefix.as_slice()) {
                        Some(((*key).into(), (*value).into()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect())
    }

    fn contains(&self, key: &DatabaseKey) -> Result<bool, Self::DatabaseError> {
        trace!("Checking if RocksDB contains: {:?}", key);
        let handle = self.db.cf_handle(key.get_cf()).expect(CF_ERROR);
        Ok(self
            .db
            .get_cf(&handle, key.as_slice())
            .map(|value| value.is_some())?)
    }

    fn remove(
        &mut self,
        key: &DatabaseKey,
        batch: Option<&mut Self::Batch>,
    ) -> Result<Option<ByteVec>, Self::DatabaseError> {
        trace!("Removing from RocksDB: {:?}", key);
        let handle = self.db.cf_handle(key.get_cf()).expect(CF_ERROR);
        let old_value = self.db.get_cf(&handle, key.as_slice())?;
        if let Some(batch) = batch {
            batch.delete_cf(&handle, key.as_slice());
        } else {
            self.db.delete_cf(&handle, key.as_slice())?;
        }
        Ok(old_value.map(Into::into))
    }

    fn remove_by_prefix(&mut self, prefix: &DatabaseKey) -> Result<(), Self::DatabaseError> {
        trace!("Getting from RocksDB: {:?}", prefix);
        let handle = self.db.cf_handle(prefix.get_cf()).expect(CF_ERROR);
        let iter = self.db.iterator_cf(
            &handle,
            IteratorMode::From(prefix.as_slice(), Direction::Forward),
        );
        let mut batch = self.create_batch();
        for kv in iter {
            if let Ok((key, _)) = kv {
                if key.starts_with(prefix.as_slice()) {
                    batch.delete_cf(&handle, &key);
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        self.write_batch(batch)?;
        Ok(())
    }

    fn write_batch(&mut self, batch: Self::Batch) -> Result<(), Self::DatabaseError> {
        Ok(self.db.write(batch)?)
    }
}

// Future thoughts: Try to factorize with the code above

impl<'db> BonsaiDatabase for RocksDBTransaction<'db> {
    type Batch = RocksDBBatch;
    type DatabaseError = RocksDBError;

    fn create_batch(&self) -> Self::Batch {
        self.txn.get_writebatch()
    }

    #[cfg(test)]
    fn dump_database(&self) {
        let handle_trie = self.column_families.get(TRIE_CF).expect(CF_ERROR);
        let handle_flat = self.column_families.get(FLAT_CF).expect(CF_ERROR);
        let handle_trie_log = self.column_families.get(TRIE_LOG_CF).expect(CF_ERROR);
        let mut iter = self.txn.raw_iterator_cf(handle_trie);
        iter.seek_to_first();
        while iter.valid() {
            let key = iter.key().unwrap();
            let value = iter.value().unwrap();
            println!("{:?} {:?}", key, value);
            iter.next();
        }
        let mut iter = self.txn.raw_iterator_cf(handle_flat);
        iter.seek_to_first();
        while iter.valid() {
            let key = iter.key().unwrap();
            let value = iter.value().unwrap();
            println!("{:?} {:?}", key, value);
            iter.next();
        }
        let mut iter = self.txn.raw_iterator_cf(handle_trie_log);
        iter.seek_to_first();
        while iter.valid() {
            let key = iter.key().unwrap();
            let value = iter.value().unwrap();
            println!("{:?} {:?}", key, value);
            iter.next();
        }
    }

    fn insert(
        &mut self,
        key: &DatabaseKey,
        value: &[u8],
        batch: Option<&mut Self::Batch>,
    ) -> Result<Option<ByteVec>, Self::DatabaseError> {
        trace!("Inserting into RocksDB: {:?} {:?}", key, value);
        let handle_cf = self.column_families.get(key.get_cf()).expect(CF_ERROR);
        let old_value = self
            .txn
            .get_cf_opt(handle_cf, key.as_slice(), &self.read_options)?;
        if let Some(batch) = batch {
            batch.put_cf(handle_cf, key.as_slice(), value);
        } else {
            self.txn.put_cf(handle_cf, key.as_slice(), value)?;
        }
        Ok(old_value.map(Into::into))
    }

    fn get(&self, key: &DatabaseKey) -> Result<Option<ByteVec>, Self::DatabaseError> {
        trace!("Getting from RocksDB: {:?}", key);
        let handle = self.column_families.get(key.get_cf()).expect(CF_ERROR);
        Ok(self
            .txn
            .get_cf_opt(handle, key.as_slice(), &self.read_options)?
            .map(Into::into))
    }

    fn get_by_prefix(
        &self,
        prefix: &DatabaseKey,
    ) -> Result<Vec<(ByteVec, ByteVec)>, Self::DatabaseError> {
        trace!("Getting from RocksDB: {:?}", prefix);
        let handle = self.column_families.get(prefix.get_cf()).expect(CF_ERROR);
        let iter = self.txn.iterator_cf(
            handle,
            IteratorMode::From(prefix.as_slice(), Direction::Forward),
        );
        Ok(iter
            .map_while(|kv| {
                if let Ok((key, value)) = kv {
                    if key.starts_with(prefix.as_slice()) {
                        Some(((*key).into(), (*value).into()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect())
    }

    fn contains(&self, key: &DatabaseKey) -> Result<bool, Self::DatabaseError> {
        trace!("Checking if RocksDB contains: {:?}", key);
        let handle = self.column_families.get(key.get_cf()).expect(CF_ERROR);
        Ok(self
            .txn
            .get_cf_opt(handle, key.as_slice(), &self.read_options)
            .map(|value| value.is_some())?)
    }

    fn remove(
        &mut self,
        key: &DatabaseKey,
        batch: Option<&mut Self::Batch>,
    ) -> Result<Option<ByteVec>, Self::DatabaseError> {
        trace!("Removing from RocksDB: {:?}", key);
        let handle = self.column_families.get(key.get_cf()).expect(CF_ERROR);
        let old_value = self
            .txn
            .get_cf_opt(handle, key.as_slice(), &self.read_options)?;
        if let Some(batch) = batch {
            batch.delete_cf(handle, key.as_slice());
        } else {
            self.txn.delete_cf(handle, key.as_slice())?;
        }
        Ok(old_value.map(Into::into))
    }

    fn remove_by_prefix(&mut self, prefix: &DatabaseKey) -> Result<(), Self::DatabaseError> {
        trace!("Getting from RocksDB: {:?}", prefix);
        let mut batch = self.create_batch();
        {
            let handle = self.column_families.get(prefix.get_cf()).expect(CF_ERROR);
            let iter = self.txn.iterator_cf(
                handle,
                IteratorMode::From(prefix.as_slice(), Direction::Forward),
            );
            for kv in iter {
                if let Ok((key, _)) = kv {
                    if key.starts_with(prefix.as_slice()) {
                        batch.delete_cf(handle, &key);
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        self.write_batch(batch)?;
        Ok(())
    }

    fn write_batch(&mut self, batch: Self::Batch) -> Result<(), Self::DatabaseError> {
        Ok(self.txn.rebuild_from_writebatch(&batch)?)
    }
}

impl<'db, ID> BonsaiPersistentDatabase<ID> for RocksDB<'db, ID>
where
    ID: Id,
{
    type Transaction<'a>
        = RocksDBTransaction<'a>
    where
        Self: 'a;
    type DatabaseError = RocksDBError;

    fn snapshot(&mut self, id: ID) {
        trace!("Generating RocksDB transaction");
        let snapshot = self.db.snapshot();
        self.snapshots.insert(id, snapshot);
        if let Some(max_number_snapshot) = self.config.max_saved_snapshots {
            while self.snapshots.len() > max_number_snapshot {
                self.snapshots.pop_first();
            }
        }
    }

    fn transaction(&self, id: ID) -> Option<(ID, Self::Transaction<'_>)> {
        trace!("Generating RocksDB transaction");
        if let Some((id, snapshot)) = self.snapshots.range(..&id).next() {
            let write_opts = WriteOptions::default();
            let mut txn_opts = OptimisticTransactionOptions::default();
            txn_opts.set_snapshot(true);
            let txn = self.db.transaction_opt(&write_opts, &txn_opts);

            let mut read_options = ReadOptions::default();
            read_options.set_snapshot(snapshot);

            let mut column_families = HashMap::new();
            column_families.insert(
                TRIE_LOG_CF.to_string(),
                self.db.cf_handle(TRIE_LOG_CF).expect(CF_ERROR),
            );
            column_families.insert(
                TRIE_CF.to_string(),
                self.db.cf_handle(TRIE_CF).expect(CF_ERROR),
            );
            column_families.insert(
                FLAT_CF.to_string(),
                self.db.cf_handle(FLAT_CF).expect(CF_ERROR),
            );
            let boxed_txn = RocksDBTransaction {
                txn,
                column_families,
                read_options,
            };
            Some((*id, boxed_txn))
        } else {
            None
        }
    }

    fn merge<'a>(&mut self, transaction: Self::Transaction<'a>) -> Result<(), Self::DatabaseError>
    where
        Self: 'a,
    {
        transaction.txn.commit()?;
        Ok(())
    }
}
