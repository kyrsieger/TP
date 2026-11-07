use crate::{common::Result, store::Store, Snapshot};
use std::path::PathBuf;

pub fn initialize(dir: PathBuf) -> Result<Store> {
    Store::open(dir)
}
pub fn test_store() -> Store {
    initialize(std::env::temp_dir().join(format!("tp-test-{}", uuid::Uuid::new_v4()))).unwrap()
}
pub fn snapshot(store: &Store) -> Result<Snapshot> {
    store.snapshot()
}
