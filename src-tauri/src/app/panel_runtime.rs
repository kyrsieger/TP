use tauri::WebviewWindow;

#[cfg(windows)]
pub(super) fn suspend_panel(
    window: &WebviewWindow,
    finished: impl FnOnce(bool) + Send + 'static,
) -> tauri::Result<()> {
    use std::sync::{Arc, Mutex};
    use webview2_com::{
        Microsoft::Web::WebView2::Win32::ICoreWebView2_3, TrySuspendCompletedHandler,
    };
    use windows::core::Interface;
    type Completion = Arc<Mutex<Option<Box<dyn FnOnce(bool) + Send>>>>;
    fn complete(completion: &Completion, succeeded: bool) {
        let finished = completion.lock().ok().and_then(|mut value| value.take());
        if let Some(finished) = finished {
            finished(succeeded);
        }
    }
    let completion: Completion = Arc::new(Mutex::new(Some(Box::new(finished))));
    window.with_webview(move |view| unsafe {
        let result = (|| -> windows::core::Result<()> {
            let controller = view.controller();
            let core = controller.CoreWebView2()?.cast::<ICoreWebView2_3>()?;
            controller.SetIsVisible(false)?;
            let callback = completion.clone();
            core.TrySuspend(&TrySuspendCompletedHandler::create(Box::new(
                move |result, succeeded| {
                    if let Err(error) = &result {
                        eprintln!("Failed to suspend panel: {error}");
                    }
                    complete(&callback, result.is_ok() && succeeded);
                    Ok(())
                },
            )))
        })();
        if let Err(error) = result {
            eprintln!("Failed to request panel suspension: {error}");
            complete(&completion, false);
        }
    })
}

#[cfg(windows)]
pub(super) fn resume_panel(window: &WebviewWindow) -> tauri::Result<()> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
    use windows::core::Interface;
    window.with_webview(|view| unsafe {
        let controller = view.controller();
        if let Ok(core) = controller
            .CoreWebView2()
            .and_then(|core| core.cast::<ICoreWebView2_3>())
        {
            if let Err(error) = core.Resume() {
                eprintln!("Failed to resume panel: {error}");
            }
        }
        // Native window visibility does not restore a manually hidden WebView2 controller.
        if let Err(error) = controller.SetIsVisible(true) {
            eprintln!("Failed to restore panel rendering: {error}");
        }
    })
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{rc::Retained, runtime::NSObjectProtocol, sel};
    use objc2_app_kit::NSView;
    use objc2_web_kit::{WKInactiveSchedulingPolicy, WKWebView};
    use std::cell::RefCell;
    thread_local! {
        static DETACHED: RefCell<Option<(Retained<NSView>, WKInactiveSchedulingPolicy)>> = const { RefCell::new(None) };
    }

    pub(super) unsafe fn suspend(pointer: *mut std::ffi::c_void) -> bool {
        let Some(view) = (unsafe { (pointer as *mut WKWebView).as_ref() }) else {
            return false;
        };
        let preferences = unsafe { view.configuration().preferences() };
        // This public API is available on macOS 14+. Older systems retain the cleared hidden panel.
        if !preferences.respondsToSelector(sel!(setInactiveSchedulingPolicy:)) {
            return false;
        }
        let Some(parent) = (unsafe { view.superview() }) else {
            return false;
        };
        let policy = unsafe { preferences.inactiveSchedulingPolicy() };
        unsafe {
            preferences.setInactiveSchedulingPolicy(WKInactiveSchedulingPolicy::Suspend);
        }
        DETACHED.with(|slot| {
            slot.replace(Some((parent, policy)));
        });
        // WebKit applies the inactive scheduling policy only to views detached from their window.
        view.removeFromSuperview();
        true
    }

    pub(super) unsafe fn resume(pointer: *mut std::ffi::c_void) {
        let Some(view) = (unsafe { (pointer as *mut WKWebView).as_ref() }) else {
            return;
        };
        if let Some((parent, policy)) = DETACHED.with(|slot| slot.take()) {
            unsafe {
                view.configuration()
                    .preferences()
                    .setInactiveSchedulingPolicy(policy);
            }
            parent.addSubview(view);
        }
    }

    pub(super) fn forget() {
        DETACHED.with(|slot| {
            slot.take();
        });
    }
}

#[cfg(target_os = "macos")]
pub(super) fn suspend_panel(
    window: &WebviewWindow,
    finished: impl FnOnce(bool) + Send + 'static,
) -> tauri::Result<()> {
    window.with_webview(move |view| {
        finished(unsafe { macos::suspend(view.inner()) });
    })
}

#[cfg(target_os = "macos")]
pub(super) fn resume_panel(window: &WebviewWindow) -> tauri::Result<()> {
    window.with_webview(|view| unsafe {
        macos::resume(view.inner());
    })
}

pub(super) fn forget_panel() {
    #[cfg(target_os = "macos")]
    macos::forget();
}
