#![allow(dead_code)]
mod hashmap_db;
pub use hashmap_db::HashMapDb;

#[cfg(feature = "rocksdb")]
mod rocks_db;

#[cfg(feature = "rocksdb")]
pub use rocks_db::{create_rocks_db, RocksDB, RocksDBBatch, RocksDBConfig};
