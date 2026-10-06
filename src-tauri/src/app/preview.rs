#[cfg(windows)]
use super::webview::filter_browser_context_menu;
use super::{
    common::{err, Result},
    webview::set_window_memory_mode,
};
use tauri::{Emitter, Manager};
#[tauri::command]
pub(super) async fn ensure_preview_window(app: tauri::AppHandle, label: String) -> Result<()> {
    if !matches!(label.as_str(), "preview" | "large-preview") {
        return Err("无效的预览窗口".into());
    }
    if app.get_webview_window(&label).is_none() {
        let config = app
            .config()
            .app
            .windows
            .iter()
            .find(|config| config.label == label)
            .ok_or("预览窗口配置不存在")?;
        let window = tauri::WebviewWindowBuilder::from_config(&app, config)
            .map_err(err)?
            .build()
            .map_err(err)?;
        #[cfg(windows)]
        filter_browser_context_menu(&window).map_err(err)?;
        #[cfg(not(windows))]
        let _ = window;
    }
    app.emit_to("main", "preview-window-used", &label)
        .map_err(err)
}

#[tauri::command]
pub(super) async fn release_preview_window(app: tauri::AppHandle, label: String) -> Result<bool> {
    if !matches!(label.as_str(), "preview" | "large-preview") {
        return Err("无效的预览窗口".into());
    }
    if let Some(window) = app.get_webview_window(&label) {
        if !window.is_visible().map_err(err)? {
            window.destroy().map_err(err)?;
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn hide_preview_window(app: &tauri::AppHandle, label: &str) -> Result<()> {
    if let Some(window) = app.get_webview_window(label) {
        window.hide().map_err(err)?;
        set_window_memory_mode(&window, true);
        let _ = app.emit_to(label, &format!("{label}-clear"), ());
        if label == "large-preview" {
            let _ = app.emit_to("main", "large-preview-closed", ());
        }
        app.emit_to("main", "preview-window-hidden", label)
            .map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub(super) fn show_preview(
    app: tauri::AppHandle,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<()> {
    let preview = app.get_webview_window("preview").ok_or("预览窗口不存在")?;
    preview
        .set_size(tauri::PhysicalSize::new(width, height))
        .map_err(err)?;
    preview
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(err)?;
    set_window_memory_mode(&preview, false);
    preview.show().map_err(err)?;
    app.emit_to("main", "preview-window-used", "preview")
        .map_err(err)
}

#[tauri::command]
pub(super) fn hide_preview(app: tauri::AppHandle) -> Result<()> {
    let focused = app
        .get_webview_window("preview")
        .map(|preview| preview.is_focused().map_err(err))
        .transpose()?
        .unwrap_or(false);
    hide_preview_window(&app, "preview")?;
    if focused {
        app.get_webview_window("main")
            .ok_or("面板窗口不存在")?
            .set_focus()
            .map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub(super) fn preview_focused(app: tauri::AppHandle) -> Result<bool> {
    app.get_webview_window("preview")
        .map(|preview| preview.is_focused().map_err(err))
        .transpose()
        .map(|focused| focused.unwrap_or(false))
}

#[tauri::command]
pub(super) fn show_large_preview(
    app: tauri::AppHandle,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<()> {
    let large = app
        .get_webview_window("large-preview")
        .ok_or("大图预览窗口不存在")?;
    large
        .set_size(tauri::PhysicalSize::new(width, height))
        .map_err(err)?;
    large
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(err)?;
    set_window_memory_mode(&large, false);
    large.show().map_err(err)?;
    app.emit_to("main", "preview-window-used", "large-preview")
        .map_err(err)?;
    large.set_focus().map_err(err)
}

#[tauri::command]
pub(super) fn hide_large_preview(app: tauri::AppHandle) -> Result<()> {
    hide_preview_window(&app, "large-preview")?;
    app.get_webview_window("main")
        .ok_or("面板窗口不存在")?
        .set_focus()
        .map_err(err)
}
