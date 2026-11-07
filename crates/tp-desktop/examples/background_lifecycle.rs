//! Desktop-session smoke check. Does not write to the system clipboard.
use std::{sync::Arc, time::Duration};
use tp_desktop::{background::Background, Store};

fn main() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("tp-background-smoke-{}", uuid::Uuid::new_v4()));
    let store = Arc::new(Store::open(dir.clone())?);
    let background = Background::start(store.clone())?;
    std::thread::sleep(Duration::from_millis(250));
    drop(background);
    assert_eq!(
        Arc::strong_count(&store),
        1,
        "background workers still own the store"
    );
    drop(store);
    std::fs::remove_dir_all(dir).map_err(|error| error.to_string())?;
    println!("Clipboard listener and maintenance workers stopped and released the store.");
    Ok(())
}
