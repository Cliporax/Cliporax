//! Preview window commands

use serde::Serialize;
use std::sync::Mutex;
use tauri::{Emitter, Manager, WebviewWindowBuilder};

const MAX_IMAGE_DATA_BYTES: usize = 50 * 1024 * 1024;
const PREVIEW_WINDOW_LABEL: &str = "preview-image";

static PREVIEW_WINDOW_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
lazy_static::lazy_static! {
    static ref PREVIEW_DATA: Mutex<Option<PreviewData>> = Mutex::new(None);
}

#[derive(Clone, Serialize)]
pub struct PreviewData {
    pub image_data: String,
    pub title: String,
}

/// Create a new preview window for displaying an image
#[tauri::command]
pub async fn preview_create_window(
    app: tauri::AppHandle,
    image_data: String,
    title: String,
) -> Result<String, String> {
    log::info!("[Command] preview_create_window called - title: {}", title);
    validate_preview_input(&image_data, &title)?;
    let _guard = PREVIEW_WINDOW_LOCK.lock().await;
    set_preview_data(&image_data, &title)?;

    if let Some(window) = app.get_webview_window(PREVIEW_WINDOW_LABEL) {
        window.set_title(&title).map_err(|e| {
            log::error!("[Command] Failed to set preview title: {}", e);
            format!("Failed to set preview title: {}", e)
        })?;
        if let Err(e) = window.emit("preview:updated", ()) {
            log::warn!("[Command] Failed to notify preview window update: {}", e);
        }
        if let Err(e) = window.set_focus() {
            log::warn!("[Command] Failed to focus preview window: {}", e);
        }
        log::info!("[Command] Preview window updated: {}", PREVIEW_WINDOW_LABEL);
        return Ok(PREVIEW_WINDOW_LABEL.to_string());
    }

    match WebviewWindowBuilder::new(
        &app,
        PREVIEW_WINDOW_LABEL,
        tauri::WebviewUrl::App("/preview".into()),
    )
    .title(&title)
    .inner_size(800.0, 600.0)
    .min_inner_size(300.0, 200.0)
    .decorations(true)
    .resizable(true)
    .always_on_top(true)
    .center()
    .build()
    {
        Ok(_) => {
            log::info!("[Command] Preview window created: {}", PREVIEW_WINDOW_LABEL);
            Ok(PREVIEW_WINDOW_LABEL.to_string())
        }
        Err(e) => {
            log::error!("[Command] Failed to create preview window: {}", e);
            Err(e.to_string())
        }
    }
}

/// Get the current image data for the preview window.
#[tauri::command]
pub async fn preview_get_data() -> Result<PreviewData, String> {
    PREVIEW_DATA
        .lock()
        .map_err(|e| format!("Failed to lock preview data: {}", e))?
        .clone()
        .ok_or_else(|| "Preview data is not available".to_string())
}

/// Close a specific preview window
#[tauri::command]
pub async fn preview_close_window(app: tauri::AppHandle, label: String) -> Result<(), String> {
    log::info!("[Command] preview_close_window: {}", label);
    if let Some(window) = app.get_webview_window(&label) {
        window.close().map_err(|e| {
            log::error!("[Command] Failed to close window: {}", e);
            e.to_string()
        })
    } else {
        log::warn!("[Command] Window {} not found", label);
        Ok(())
    }
}

/// Close all preview windows
#[tauri::command]
pub async fn preview_close_all(app: tauri::AppHandle) -> Result<(), String> {
    log::info!("[Command] preview_close_all called");
    let windows = app.webview_windows();
    for (label, window) in windows.iter() {
        if label.starts_with("preview-") {
            let _ = window.close();
            log::info!("[Command] Closed preview window: {}", label);
        }
    }
    Ok(())
}

fn validate_preview_input(image_data: &str, title: &str) -> Result<(), String> {
    let trimmed_title = title.trim();
    if trimmed_title.is_empty() {
        return Err("Preview title cannot be empty".to_string());
    }

    if trimmed_title.len() > 200 {
        return Err("Preview title is too long".to_string());
    }

    if image_data.is_empty() {
        return Err("Preview image data cannot be empty".to_string());
    }

    if image_data.len() > MAX_IMAGE_DATA_BYTES {
        return Err("Preview image data is too large".to_string());
    }

    if !image_data.starts_with("data:image/") {
        return Err("Preview image data must be a data:image URL".to_string());
    }

    Ok(())
}

fn set_preview_data(image_data: &str, title: &str) -> Result<(), String> {
    let mut preview_data = PREVIEW_DATA
        .lock()
        .map_err(|e| format!("Failed to lock preview data: {}", e))?;
    *preview_data = Some(PreviewData {
        image_data: image_data.to_string(),
        title: title.to_string(),
    });
    Ok(())
}
