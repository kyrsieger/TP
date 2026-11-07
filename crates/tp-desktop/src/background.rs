use crate::{
    clipboard::Watcher,
    common::{err, Result},
    platform, Changed, Store,
};
use clipboard_rs::{ClipboardContext, ClipboardWatcher, ClipboardWatcherContext, WatcherShutdown};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// Owns the clipboard listener independently of whether a panel exists or is visible.
pub struct Background {
    pub revision: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    shutdown: Option<WatcherShutdown>,
    watcher: Option<JoinHandle<()>>,
    maintenance: Option<JoinHandle<()>>,
}

impl Background {
    pub fn start(store: Arc<Store>) -> Result<Self> {
        let revision = Arc::new(AtomicU64::new(0));
        let changes = revision.clone();
        let changed: Changed = Arc::new(move || {
            changes.fetch_add(1, Ordering::Release);
        });
        let capture_store = store.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        let watcher = thread::spawn(move || {
            let initialized = (|| -> Result<_> {
                let ctx = ClipboardContext::new().map_err(err)?;
                let mut watcher = ClipboardWatcherContext::new().map_err(err)?;
                watcher.add_handler(Watcher {
                    store: capture_store,
                    changed,
                    ctx,
                });
                Ok(watcher)
            })();
            match initialized {
                Ok(mut watcher) => {
                    if tx.send(Ok(watcher.get_shutdown_channel())).is_ok() {
                        watcher.start_watch();
                    }
                }
                Err(error) => {
                    let _ = tx.send(Err(error));
                }
            }
        });
        let shutdown = match rx.recv().map_err(err)? {
            Ok(shutdown) => shutdown,
            Err(error) => {
                let _ = watcher.join();
                return Err(error);
            }
        };
        let stop = Arc::new(AtomicBool::new(false));
        let maintenance_stop = stop.clone();
        let changes = revision.clone();
        let maintenance = thread::spawn(move || {
            let mut ticks = 0;
            while !maintenance_stop.load(Ordering::Acquire) {
                if let Some(target) = platform::capture_paste_target() {
                    if let Ok(mut current) = store.paste_target.lock() {
                        *current = Some(target);
                    }
                }
                if ticks % 10 == 0 {
                    if let Ok(_data) = store.read_data() {
                        if matches!(tp_core::cleanup::clean(&store), Ok(true)) {
                            changes.fetch_add(1, Ordering::Release);
                        }
                    }
                    if tp_core::i18n::refresh_system() {
                        changes.fetch_add(1, Ordering::Release);
                    }
                }
                ticks = (ticks + 1) % 10;
                thread::sleep(Duration::from_millis(100));
            }
        });
        Ok(Self {
            revision,
            stop,
            shutdown: Some(shutdown),
            watcher: Some(watcher),
            maintenance: Some(maintenance),
        })
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(shutdown) = self.shutdown.take() {
            shutdown.stop();
        }
        if let Some(worker) = self.watcher.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.maintenance.take() {
            let _ = worker.join();
        }
    }
}
