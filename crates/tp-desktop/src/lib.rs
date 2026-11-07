//! Native clipboard, files and media services shared by desktop hosts.
pub mod autostart;
pub mod background;
pub mod clipboard;
mod clipboard_diagnostics;
mod clipboard_text;
pub mod files;
pub mod fonts;
pub mod media;
pub mod native_preview;
pub mod platform;
pub use tp_core::records as model;
pub use tp_core::{cleanup, common, digest, filesystem, i18n};

pub type Store = tp_core::store::Store<platform::PasteTarget>;
pub type Changed = std::sync::Arc<dyn Fn() + Send + Sync>;

pub mod storage {
    pub use crate::Store;
    pub use tp_core::storage::read_settings as settings;
    pub fn snapshot(store: &Store) -> tp_core::common::Result<tp_core::Snapshot> {
        store.snapshot()
    }
}

#[cfg(test)]
mod test_support {
    pub fn test_store() -> crate::Store {
        crate::Store::open(std::env::temp_dir().join(format!("tp-test-{}", uuid::Uuid::new_v4())))
            .unwrap()
    }
}
#[cfg(test)]
mod clipboard_text_tests;
