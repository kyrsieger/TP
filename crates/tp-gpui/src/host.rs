use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::{cell::Cell, sync::Arc};
use tp_core::common::{err, Result};
use tp_desktop::{background::Background, Store};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

pub struct Host {
    pub background: Background,
    pub startup_error: Option<String>,
    _tray: TrayIcon,
    hotkeys: GlobalHotKeyManager,
    shortcut: Option<HotKey>,
    open_label: MenuItem,
    quit_label: MenuItem,
    chinese: Cell<bool>,
}
impl gpui::Global for Host {}

impl Host {
    pub fn new(store: Arc<Store>, shortcut: &str) -> Result<Self> {
        let menu = Menu::new();
        let open = MenuItem::with_id("open", tp_core::i18n::tr("打开剪贴板"), true, None);
        let quit = MenuItem::with_id("quit", tp_core::i18n::tr("退出"), true, None);
        menu.append_items(&[&open, &quit]).map_err(err)?;
        #[cfg(windows)]
        let bytes = include_bytes!("../../../packaging/icons/tray-windows.png");
        #[cfg(target_os = "macos")]
        let bytes = include_bytes!("../../../packaging/icons/tray-template.png");
        let pixels = image::load_from_memory(bytes).map_err(err)?.into_rgba8();
        let (width, height) = pixels.dimensions();
        let builder = TrayIconBuilder::new()
            .with_tooltip("TP")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_icon(Icon::from_rgba(pixels.into_raw(), width, height).map_err(err)?);
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_as_template(true);
        let tray = builder.build().map_err(err)?;
        let hotkeys = GlobalHotKeyManager::new().map_err(err)?;
        let mut startup_error = None;
        let shortcut = if shortcut.is_empty() {
            None
        } else {
            match shortcut
                .parse::<HotKey>()
                .map_err(err)
                .and_then(|key| hotkeys.register(key).map_err(err).map(|_| key))
            {
                Ok(key) => Some(key),
                Err(error) => {
                    startup_error = Some(error);
                    None
                }
            }
        };
        let background = Background::start(store)?;
        Ok(Self {
            background,
            startup_error,
            _tray: tray,
            hotkeys,
            shortcut,
            open_label: open,
            quit_label: quit,
            chinese: Cell::new(tp_core::i18n::chinese()),
        })
    }

    /// Call on the UI thread, as required by the native hotkey manager.
    pub fn replace_shortcut(&mut self, value: &str) -> Result<()> {
        let next = if value.is_empty() {
            None
        } else {
            Some(value.parse::<HotKey>().map_err(err)?)
        };
        replace_registered(
            &mut self.shortcut,
            next,
            |key| self.hotkeys.register(key).map_err(err),
            |key| self.hotkeys.unregister(key).map_err(err),
        )?;
        self.startup_error = None;
        Ok(())
    }

    pub fn poll(&self) -> (bool, bool) {
        let chinese = tp_core::i18n::chinese();
        if self.chinese.replace(chinese) != chinese {
            self.open_label.set_text(tp_core::i18n::tr("打开剪贴板"));
            self.quit_label.set_text(tp_core::i18n::tr("退出"));
        }
        let (mut open, mut quit) = (false, false);
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id.as_ref() {
                "open" => open = true,
                "quit" => quit = true,
                _ => {}
            }
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                open = true;
            }
        }
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state() == HotKeyState::Pressed
                && self.shortcut.is_some_and(|key| key.id() == event.id())
            {
                open = true;
            }
        }
        (open, quit)
    }
}

/// Keep the working shortcut until its replacement has been registered.
fn replace_registered(
    active: &mut Option<HotKey>,
    next: Option<HotKey>,
    mut register: impl FnMut(HotKey) -> Result<()>,
    mut unregister: impl FnMut(HotKey) -> Result<()>,
) -> Result<()> {
    if *active == next {
        return Ok(());
    }
    if let Some(key) = next {
        register(key)?;
    }
    if let Some(previous) = *active {
        if let Err(error) = unregister(previous) {
            if let Some(key) = next {
                if let Err(rollback) = unregister(key) {
                    return Err(format!("{error}; {rollback}"));
                }
            }
            return Err(error);
        }
    }
    *active = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[test]
    fn failed_replacement_keeps_old_key_and_success_registers_before_releasing_it() {
        let old = "Control+Alt+F11".parse::<HotKey>().unwrap();
        let new = "Control+Alt+F12".parse::<HotKey>().unwrap();
        let mut active = Some(old);
        assert!(replace_registered(
            &mut active,
            Some(new),
            |_| Err("occupied".into()),
            |_| panic!("old key must remain registered")
        )
        .is_err());
        assert_eq!(active, Some(old));
        let events = RefCell::new(Vec::new());
        replace_registered(
            &mut active,
            Some(new),
            |key| {
                events.borrow_mut().push((true, key));
                Ok(())
            },
            |key| {
                events.borrow_mut().push((false, key));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*events.borrow(), vec![(true, new), (false, old)]);
        assert_eq!(active, Some(new));
        replace_registered(
            &mut active,
            None,
            |_| panic!("clearing cannot register"),
            |key| {
                assert_eq!(key, new);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(active, None);
    }
    #[test]
    fn failed_old_key_release_removes_new_registration() {
        let old = "Control+F11".parse::<HotKey>().unwrap();
        let new = "Control+F12".parse::<HotKey>().unwrap();
        let mut active = Some(old);
        let mut removed = Vec::new();
        assert!(replace_registered(
            &mut active,
            Some(new),
            |_| Ok(()),
            |key| {
                removed.push(key);
                if key == old {
                    Err("release failed".into())
                } else {
                    Ok(())
                }
            }
        )
        .is_err());
        assert_eq!(active, Some(old));
        assert_eq!(removed, vec![old, new]);
    }
}
