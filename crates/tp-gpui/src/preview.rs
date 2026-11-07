//! UI-thread presentation with worker decoding and cancellation of stale loads.
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, Sender},
    Arc,
};
use tp_desktop::{
    native_preview::{self as native, Callbacks, Picture},
    Store,
};

enum Event {
    Navigate(u64, String, usize),
    Loaded(u64, Result<Picture, String>),
    Closed(u64),
}

fn trace(message: impl std::fmt::Display) {
    if cfg!(debug_assertions) && std::env::var_os("TP_DIAGNOSTIC_PREVIEW").is_some() {
        eprintln!("large-preview: {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_cancels_queued_navigation_and_late_results_before_next_ui_poll() {
        let dir = std::env::temp_dir().join(format!(
            "tp-preview-cancel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Arc::new(Store::open(dir.clone()).unwrap());
        let mut preview = Preview::new(store.clone(), 0);
        preview.active = Some("no-record".into());
        (preview.callbacks.navigate)("no-record".into(), 1);
        let loading = preview.generation.load(Ordering::SeqCst);
        (preview.callbacks.closed)();
        preview
            .sender
            .send(Event::Loaded(loading, Err("late decode failure".into())))
            .unwrap();
        assert!(preview.poll().unwrap());
        assert!(!preview.is_active());
        assert!(preview.receiver.try_recv().is_err());

        // Reopening before the old close notification is consumed retains the new session.
        (preview.callbacks.closed)();
        preview.generation.fetch_add(1, Ordering::SeqCst);
        preview.active = Some("no-record".into());
        assert!(!preview.poll().unwrap());
        assert!(preview.is_active());

        let loading = preview.generation.load(Ordering::SeqCst);
        preview.hide();
        preview
            .sender
            .send(Event::Loaded(loading, Err("panel was hidden".into())))
            .unwrap();
        assert!(!preview.poll().unwrap());
        assert!(!preview.is_active());
        drop(preview);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn removed_record_cancels_pending_results_but_retained_record_keeps_session() {
        let dir = std::env::temp_dir().join(format!(
            "tp-preview-remove-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Arc::new(Store::open(dir.clone()).unwrap());
        let mut preview = Preview::new(store.clone(), 0);
        preview.active = Some("removed".into());
        let ticket = preview.generation.load(Ordering::SeqCst);
        preview.reconcile(["unrelated", "removed"]);
        assert!(preview.is_active());
        assert_eq!(preview.generation.load(Ordering::SeqCst), ticket);
        preview.reconcile(["unrelated"]);
        assert!(!preview.is_active());
        assert_ne!(preview.generation.load(Ordering::SeqCst), ticket);
        preview
            .sender
            .send(Event::Loaded(
                ticket,
                Err("late result for deleted record".into()),
            ))
            .unwrap();
        assert!(!preview.poll().unwrap());
        preview.active = Some("replacement".into());
        preview.reconcile(["replacement"]);
        assert!(preview.is_active());
        preview.reconcile(std::iter::empty());
        assert!(!preview.is_active());
        drop(preview);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

pub struct Preview {
    store: Arc<Store>,
    generation: Arc<AtomicU64>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    callbacks: Callbacks,
    active: Option<String>,
    chinese: bool,
}
impl Preview {
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }
    pub fn new(store: Arc<Store>, owner: usize) -> Self {
        let generation = Arc::new(AtomicU64::new(0));
        let (sender, receiver) = mpsc::channel();
        let next = generation.clone();
        let closed = generation.clone();
        let navigate_tx = sender.clone();
        let closed_tx = sender.clone();
        let callbacks = Callbacks {
            owner,
            navigate: Arc::new(move |id, index| {
                let ticket = next.fetch_add(1, Ordering::SeqCst) + 1;
                let _ = navigate_tx.send(Event::Navigate(ticket, id, index));
            }),
            closed: Arc::new(move || {
                let ticket = closed.fetch_add(1, Ordering::SeqCst) + 1;
                let _ = closed_tx.send(Event::Closed(ticket));
            }),
        };
        Self {
            store,
            generation,
            sender,
            receiver,
            callbacks,
            active: None,
            chinese: tp_core::i18n::chinese(),
        }
    }
    fn load(&self, ticket: u64, id: String, index: usize) -> Result<(), String> {
        let store = self.store.clone();
        let sender = self.sender.clone();
        std::thread::Builder::new()
            .name("tp-preview".into())
            .spawn(move || {
                let result = native::load(&store, id, Some(index));
                let _ = sender.send(Event::Loaded(ticket, result));
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn open(&mut self, id: String) -> Result<(), String> {
        if self.is_active() {
            return Ok(());
        }
        let ticket = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        trace(format_args!("opening generation {ticket}"));
        self.load(ticket, id.clone(), 0)?;
        self.active = Some(id);
        Ok(())
    }
    /// Keep a preview only while its record is present in the full snapshot.
    /// Search/type filters do not change ownership of the large preview.
    pub fn reconcile<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) {
        let Some(active) = self.active.as_deref() else {
            return;
        };
        if !ids.into_iter().any(|id| id == active) {
            self.hide();
        }
    }
    #[track_caller]
    pub fn hide(&mut self) {
        trace(format_args!(
            "hide requested at {}",
            std::panic::Location::caller()
        ));
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.active = None;
        native::hide();
    }
    /// Returns true when the panel should regain focus after the user closes preview.
    pub fn poll(&mut self) -> Result<bool, String> {
        let chinese = tp_core::i18n::chinese();
        if self.chinese != chinese {
            self.chinese = chinese;
            native::refresh_language();
        }
        let mut focus = false;
        while let Ok(event) = self.receiver.try_recv() {
            let ticket = match &event {
                Event::Navigate(t, ..) | Event::Loaded(t, _) | Event::Closed(t) => *t,
            };
            if ticket != self.generation.load(Ordering::SeqCst) {
                continue;
            }
            let result = match event {
                Event::Navigate(ticket, id, index) => {
                    if self.active.as_deref() == Some(id.as_str()) {
                        self.load(ticket, id, index)
                    } else {
                        Ok(())
                    }
                }
                Event::Loaded(_, picture) => picture.and_then(|picture| {
                    trace(format_args!(
                        "loaded {}x{}",
                        picture.image.width(),
                        picture.image.height()
                    ));
                    let result = native::show(self.callbacks.clone(), picture);
                    trace(format_args!("presentation result: {result:?}"));
                    result
                }),
                Event::Closed(_) => {
                    trace("native window closed");
                    self.active = None;
                    focus = true;
                    Ok(())
                }
            };
            if let Err(error) = result {
                self.hide();
                return Err(error);
            }
        }
        Ok(focus)
    }
}
