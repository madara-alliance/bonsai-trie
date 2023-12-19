#![allow(dead_code)]
mod hashmap_db;

mod rocks_db;

pub use rocks_db::{create_rocks_db, RocksDB, RocksDBBatch, RocksDBConfig};
