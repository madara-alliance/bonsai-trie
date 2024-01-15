#![allow(dead_code)]
mod hashmap_db;

#[cfg(feature = "rocksdb")]
mod rocks_db;

#[cfg(feature = "rocksdb")]
pub use rocks_db::{create_rocks_db, RocksDB, RocksDBBatch, RocksDBConfig};
