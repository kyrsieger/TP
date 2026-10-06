use super::storage::{initialize, Store};
use uuid::Uuid;
pub(super) fn test_store() -> Store {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    initialize(dir).unwrap()
}
