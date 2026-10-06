pub(super) fn set_window_memory_mode(window: &tauri::WebviewWindow, low: bool) {
    #[cfg(windows)]
    if let Err(error) = window.with_webview(move |webview| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
        };
        use windows::core::Interface;
        let result = webview
            .controller()
            .CoreWebView2()
            .and_then(|core| core.cast::<ICoreWebView2_19>())
            .and_then(|core| {
                core.SetMemoryUsageTargetLevel(if low {
                    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
                } else {
                    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
                })
            });
        if let Err(error) = result {
            eprintln!("Failed to set WebView memory mode: {error}");
        }
    }) {
        eprintln!("Failed to access WebView: {error}");
    }
    #[cfg(not(windows))]
    let _ = (window, low);
}

#[cfg(windows)]
pub(super) fn filter_browser_context_menu(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    window.with_webview(|webview| {
        use webview2_com::{
            ContextMenuRequestedEventHandler,
            Microsoft::Web::WebView2::Win32::{
                ICoreWebView2_11, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND,
                COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
            },
        };
        use windows::core::{Interface, PWSTR};

        let result = (|| -> windows::core::Result<()> {
            let core =
                unsafe { webview.controller().CoreWebView2()? }.cast::<ICoreWebView2_11>()?;
            let handler = ContextMenuRequestedEventHandler::create(Box::new(|_, args| {
                let Some(args) = args else { return Ok(()) };
                let items = unsafe { args.MenuItems()? };
                let mut count = 0;
                unsafe { items.Count(&mut count)? };
                let mut removed = false;
                for index in (0..count).rev() {
                    let item = unsafe { items.GetValueAtIndex(index)? };
                    let mut kind = COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND::default();
                    unsafe { item.Kind(&mut kind)? };
                    if kind == COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR {
                        continue;
                    }
                    let mut name = PWSTR::null();
                    let mut label = PWSTR::null();
                    unsafe {
                        item.Name(&mut name)?;
                        item.Label(&mut label)?;
                    }
                    let name = webview2_com::take_pwstr(name).to_ascii_lowercase();
                    let label = webview2_com::take_pwstr(label).replace('&', "");
                    let label = label
                        .split(['(', '（'])
                        .next()
                        .unwrap_or(&label)
                        .trim_end_matches('…')
                        .trim()
                        .to_ascii_lowercase();
                    if matches!(name.as_str(), "moretools" | "inspect" | "inspectelement")
                        || matches!(
                            label.as_str(),
                            "更多工具"
                                | "检查"
                                | "检查元素"
                                | "more tools"
                                | "inspect"
                                | "inspect element"
                        )
                    {
                        unsafe { items.RemoveValueAtIndex(index)? };
                        removed = true;
                    }
                }
                if removed {
                    unsafe { items.Count(&mut count)? };
                    let mut next_is_separator = true;
                    for index in (0..count).rev() {
                        let item = unsafe { items.GetValueAtIndex(index)? };
                        let mut kind = COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND::default();
                        unsafe { item.Kind(&mut kind)? };
                        let separator = kind == COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR;
                        if separator && (next_is_separator || index == 0) {
                            unsafe { items.RemoveValueAtIndex(index)? };
                        }
                        next_is_separator = separator;
                    }
                }
                Ok(())
            }));
            let mut token = 0;
            unsafe { core.add_ContextMenuRequested(&handler, &mut token) }
        })();
        if let Err(error) = result {
            eprintln!("Failed to filter browser context menu: {error}");
        }
    })
}
