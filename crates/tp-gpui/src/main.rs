#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

macro_rules! impl_view_render {
    ($view:ty) => {
        impl gpui::Render for $view {
            fn render(
                &mut self,
                window: &mut gpui::Window,
                cx: &mut gpui::Context<Self>,
            ) -> impl gpui::IntoElement {
                #[cfg(all(debug_assertions, feature = "hot-reload"))]
                {
                    // A named render function with an erased element type gives patches a stable call boundary.
                    subsecond::HotFn::current(Self::render_view).call((self, window, cx))
                }
                #[cfg(not(all(debug_assertions, feature = "hot-reload")))]
                self.render_view(window, cx)
            }
        }
    };
}

mod cleanup_view;
mod font_picker;
mod highlight;
mod host;
#[cfg(all(debug_assertions, feature = "hot-reload"))]
mod hot_reload;
mod hover_card;
mod hover_lifetime;
mod icons;
#[cfg(test)]
#[path = "../../../vendor/gpui-component/src/input/utf16_limit.rs"]
mod input_limit_tests;
mod media_cache;
mod media_transition;
mod native_window;
mod panel;
mod preview;
mod selectable_text;
mod settings_reference;
mod settings_view;
mod shortcuts;
mod startup;
mod text_selection;
mod theme;
mod transfer_view;
mod updates;
mod window_preferences;
use gpui::{prelude::*, px, size, App, Application, Bounds, WindowBounds, WindowOptions};
use gpui_component::Root;
use std::{path::PathBuf, sync::Arc};
use tp_core::Snapshot;
use tp_desktop::Store;

fn load(show_panel: bool) -> Result<(Arc<Store>, Snapshot), String> {
    let dir = if cfg!(debug_assertions) {
        std::env::var_os("TP_DIAGNOSTIC_DATA_DIR").map(PathBuf::from)
    } else {
        None
    };
    let dir = dir
        .or_else(|| dirs::data_dir().map(|root| root.join("TP")))
        .ok_or_else(|| "无法定位应用数据目录".to_owned())?;
    if dir.file_name().is_some_and(|name| name == "TP") {
        tp_core::migration::migrate_data_directory(&dir.with_file_name("com.tp.clipboard"), &dir)?;
    }
    let store = Arc::new(Store::open(dir)?);
    let snapshot = if show_panel {
        store.snapshot()?
    } else {
        // A tray-only launch needs preferences, not the full clipboard history.
        let conn = store.db.lock().map_err(tp_core::common::err)?;
        Snapshot {
            settings: tp_core::storage::read_settings(&conn)?,
            records: Vec::new(),
            groups: Vec::new(),
        }
    };
    tp_core::i18n::set_preference(&snapshot.settings.language);
    let diagnostic = cfg!(debug_assertions) && std::env::var_os("TP_DIAGNOSTIC_DATA_DIR").is_some();
    if !diagnostic {
        if let Err(error) = tp_desktop::autostart::restore(&store) {
            eprintln!("Failed to restore autostart: {error}");
        }
    }
    Ok((store, snapshot))
}

fn main() {
    #[cfg(all(debug_assertions, feature = "hot-reload"))]
    let devserver = hot_reload::devserver_address();
    #[cfg(all(debug_assertions, feature = "hot-reload"))]
    let hot_reload = devserver.is_some();
    #[cfg(not(all(debug_assertions, feature = "hot-reload")))]
    let hot_reload = false;
    let show_panel = startup::show_panel(
        cfg!(debug_assertions),
        std::env::var_os("TP_DIAGNOSTIC_DATA_DIR").is_some(),
        hot_reload,
        std::env::var_os("TP_DIAGNOSTIC_START_HIDDEN").is_some(),
    );
    let (store, snapshot) = match load(show_panel) {
        Ok(data) => data,
        Err(error) => {
            rfd::MessageDialog::new()
                .set_title("TP")
                .set_description(error)
                .set_level(rfd::MessageLevel::Error)
                .show();
            return;
        }
    };
    Application::new()
        .with_assets(icons::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            #[cfg(all(debug_assertions, feature = "hot-reload"))]
            if let Some(address) = devserver {
                hot_reload::start(cx, address);
            }
            transfer_view::init(cx);
            cleanup_view::init(cx);
            let host = match host::Host::new(store.clone(), &snapshot.settings.shortcut) {
                Ok(host) => host,
                Err(error) => {
                    rfd::MessageDialog::new()
                        .set_title("TP")
                        .set_description(error)
                        .set_level(rfd::MessageLevel::Error)
                        .show();
                    cx.quit();
                    return;
                }
            };
            cx.set_global(host);
            for context in ["TP", "TP > Input"] {
                cx.bind_keys([
                    gpui::KeyBinding::new("tab", panel::NextGroup, Some(context)),
                    gpui::KeyBinding::new("shift-tab", panel::PreviousGroup, Some(context)),
                ]);
            }
            theme::Palette::apply(&snapshot.settings, cx);
            cx.bind_keys([gpui::KeyBinding::new(
                "ctrl-f",
                panel::FocusSearch,
                Some("TP"),
            )]);
            // Shortcut recording needs the physical key event, before the
            // panel-wide search action can move focus out of the recorder.
            cx.bind_keys([gpui::KeyBinding::new(
                "ctrl-f",
                gpui::NoAction,
                Some("TPShortcutCapture"),
            )]);
            let bounds = Bounds::centered(
                None,
                size(
                    px(snapshot.settings.panel_width as f32),
                    px(snapshot.settings.panel_height as f32),
                ),
                cx,
            );
            cx.open_window(
                WindowOptions {
                    show: show_panel,
                    focus: show_panel,
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(340.), px(400.))),
                    app_id: Some("TP".into()),
                    titlebar: None,
                    ..Default::default()
                },
                |window, cx| {
                    window.set_window_title("TP");
                    native_window::install_ime_guard(window).expect("无法初始化输入法按键保护");
                    let view =
                        cx.new(|cx| panel::Panel::new(store, snapshot, show_panel, window, cx));
                    let panel = view.downgrade();
                    window.on_window_should_close(cx, move |window, cx| {
                        let _ = panel.update(cx, |panel, cx| panel.hide(window, cx));
                        false
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("无法创建 TP 窗口");
            if show_panel {
                cx.activate(true);
            }
        });
}
