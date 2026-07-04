use crate::db::{ClipboardItemInput, ClipboardRepository, Db, Metadata};
use arboard::Clipboard;
use base64::{engine::general_purpose, Engine as _};
use image::{
    codecs::png::{CompressionType, FilterType, PngEncoder},
    ExtendedColorType, ImageEncoder,
};
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL};
use regex::Regex;
use serde_json;
use sha2::{Digest, Sha256};
#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Emitter;
use tokio::sync::Mutex;
use tokio::time::interval;

lazy_static::lazy_static! {
    static ref SENSITIVE_REGEX: Regex = Regex::new(r"(?i)(password|code|otp|验证码|secret|key)").unwrap();
}

#[derive(Clone, serde::Serialize)]
struct ClipboardChangedPayload {
    #[serde(rename = "tabIds")]
    tab_ids: Vec<i64>,
    #[serde(rename = "itemIds")]
    item_ids: Vec<i64>,
    reason: &'static str,
}

fn emit_clipboard_changed(
    app_handle: &tauri::AppHandle,
    tab_ids: Vec<i64>,
    item_ids: Vec<i64>,
    reason: &'static str,
) {
    let app_handle_clone = app_handle.clone();
    tokio::spawn(async move {
        let payload = ClipboardChangedPayload {
            tab_ids,
            item_ids,
            reason,
        };
        match app_handle_clone.emit("clipboard:changed", payload) {
            Ok(_) => log::info!("[Clipboard] Event emitted successfully"),
            Err(e) => log::error!("[Clipboard] Failed to emit event: {}", e),
        }
    });
}

async fn item_tab_id(db: &Db, item_id: i64) -> Option<i64> {
    sqlx::query_scalar::<_, Option<i64>>("SELECT tab_id FROM clipboard_items WHERE id = ?")
        .bind(item_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .flatten()
}

/// Convert RGBA bytes to PNG format (CPU intensive)
fn rgba_to_png(
    rgba_bytes: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let expected_len = width as usize * height as usize * 4;
    if rgba_bytes.len() != expected_len {
        return Err(format!(
            "Invalid RGBA buffer length: expected {}, got {}",
            expected_len,
            rgba_bytes.len()
        )
        .into());
    }

    let mut png_bytes = Vec::with_capacity(rgba_bytes.len().min(16 * 1024 * 1024));
    let encoder =
        PngEncoder::new_with_quality(&mut png_bytes, CompressionType::Fast, FilterType::NoFilter);
    encoder.write_image(rgba_bytes, width, height, ExtendedColorType::Rgba8)?;
    Ok(png_bytes)
}

/// Calculate SHA256 hash of image bytes
fn calculate_image_hash(rgba_bytes: &[u8], width: u32, height: u32) -> String {
    let mut hasher = Sha256::new();
    hasher.update(width.to_le_bytes());
    hasher.update(height.to_le_bytes());
    hasher.update(rgba_bytes);
    format!("{:x}", hasher.finalize())
}

fn read_clipboard_text(clipboard: &mut Clipboard) -> String {
    #[cfg(target_os = "windows")]
    {
        let _ = clipboard;
        return Clipboard::new()
            .and_then(|mut clip| clip.get_text())
            .unwrap_or_default();
    }

    #[cfg(not(target_os = "windows"))]
    {
        clipboard.get_text().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::looks_like_file_uri_list;

    #[test]
    fn recognizes_only_pure_file_uri_text() {
        assert!(looks_like_file_uri_list("file:///tmp/one\nfile:///tmp/two"));
        assert!(!looks_like_file_uri_list(
            "open file:///tmp/one in the file manager"
        ));
        assert!(!looks_like_file_uri_list(""));
    }
}

fn read_clipboard_image(clipboard: &mut Clipboard) -> Option<(usize, usize, Vec<u8>)> {
    #[cfg(target_os = "windows")]
    {
        let _ = clipboard;
        return Clipboard::new()
            .and_then(|mut clip| clip.get_image())
            .ok()
            .map(|image| (image.width, image.height, image.bytes.to_vec()));
    }

    #[cfg(not(target_os = "windows"))]
    {
        clipboard
            .get_image()
            .ok()
            .map(|image| (image.width, image.height, image.bytes.to_vec()))
    }
}

pub(crate) fn parse_file_list(content: &str) -> Vec<PathBuf> {
    if let Ok(paths) = serde_json::from_str::<Vec<String>>(content) {
        return paths
            .into_iter()
            .filter_map(|value| clipboard_path_from_value(&value))
            .filter(|path| path.exists())
            .collect();
    }

    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(clipboard_path_from_value)
        .filter(|path| path.exists())
        .collect()
}

fn clipboard_path_from_value(value: &str) -> Option<PathBuf> {
    let Some(encoded_path) = value.strip_prefix("file://") else {
        return Some(PathBuf::from(value));
    };
    let decoded = urlencoding::decode(encoded_path).ok()?.into_owned();

    #[cfg(target_os = "windows")]
    let decoded =
        if decoded.as_bytes().first() == Some(&b'/') && decoded.as_bytes().get(2) == Some(&b':') {
            decoded[1..].to_string()
        } else {
            decoded
        };

    let path = PathBuf::from(decoded);
    path.is_absolute().then_some(path)
}

fn looks_like_file_uri_list(content: &str) -> bool {
    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let Some(first) = lines.next() else {
        return false;
    };
    first.starts_with("file://") && lines.all(|line| line.starts_with("file://"))
}

#[cfg(target_os = "linux")]
fn file_uri_from_path(path: &Path) -> String {
    format!("file://{}", path.to_string_lossy().replace('\\', "/"))
}

#[cfg(target_os = "linux")]
fn paths_from_uri_list(uri_list: &str) -> Vec<String> {
    uri_list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(clipboard_path_from_value)
        .filter(|path| path.exists())
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

#[cfg(target_os = "windows")]
fn get_windows_clipboard_sequence_number() -> u32 {
    unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

#[cfg(target_os = "windows")]
async fn get_windows_file_list() -> Option<Vec<String>> {
    let mut command = tokio::process::Command::new("powershell");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OutputEncoding = [Text.UTF8Encoding]::new(); Get-Clipboard -Format FileDropList | ForEach-Object { $_.FullName }",
        ])
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_millis(800), command.output())
        .await
        .ok()?
        .ok()?;
    existing_absolute_paths(&output.stdout, output.status.success())
}

#[cfg(target_os = "macos")]
fn get_macos_clipboard_change_count() -> i64 {
    NSPasteboard::generalPasteboard().changeCount() as i64
}

#[cfg(target_os = "macos")]
fn get_macos_file_list() -> Option<Vec<String>> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let items = pasteboard.pasteboardItems()?;
    let mut paths = Vec::new();
    for item in items.iter() {
        let Some(file_url) = item.stringForType(unsafe { NSPasteboardTypeFileURL }) else {
            continue;
        };
        let Some(path) = clipboard_path_from_value(&file_url.to_string()) else {
            continue;
        };
        if path.is_absolute() && path.exists() {
            paths.push(path.to_string_lossy().into_owned());
        }
    }
    (!paths.is_empty()).then_some(paths)
}

#[cfg(target_os = "windows")]
fn existing_absolute_paths(bytes: &[u8], successful: bool) -> Option<Vec<String>> {
    if !successful || bytes.is_empty() {
        return None;
    }
    let paths = String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.exists())
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    (!paths.is_empty()).then_some(paths)
}

#[cfg(target_os = "linux")]
async fn run_xclip_with_timeout(args: &[&str], timeout_ms: u64) -> Option<std::process::Output> {
    let mut command = tokio::process::Command::new("xclip");
    command.args(args).kill_on_drop(true);

    match tokio::time::timeout(Duration::from_millis(timeout_ms), command.output()).await {
        Ok(Ok(output)) => Some(output),
        Ok(Err(e)) => {
            log::warn!("[Clipboard] Failed to run xclip {:?}: {}", args, e);
            None
        }
        Err(_) => {
            log::warn!(
                "[Clipboard] xclip {:?} timed out after {}ms",
                args,
                timeout_ms
            );
            None
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_targets_have_text(targets: &str) -> bool {
    targets.lines().any(|target| {
        matches!(
            target.trim(),
            "UTF8_STRING" | "TEXT" | "STRING" | "text/plain" | "text/plain;charset=utf-8"
        )
    })
}

#[cfg(target_os = "linux")]
fn linux_targets_have_image(targets: &str) -> bool {
    targets.lines().any(|target| {
        matches!(
            target.trim(),
            "image/png" | "image/jpeg" | "image/bmp" | "image/x-ico" | "image/webp"
        )
    })
}

#[cfg(target_os = "linux")]
struct LinuxImageRead {
    data_url: String,
    mime_type: &'static str,
    bytes_len: usize,
    elapsed_ms: u128,
}

pub struct ClipboardMonitor {
    clipboard: Arc<Mutex<Clipboard>>,
    last_text: Arc<Mutex<String>>,
    last_image_hash: Arc<Mutex<String>>,
    is_internal_change: Arc<Mutex<bool>>,
    suppressed_text_hash: Arc<Mutex<Option<String>>>,
    // Track the last processed image hash to avoid re-processing duplicates
    last_processed_image_hash: Arc<Mutex<String>>,
    #[cfg(target_os = "windows")]
    last_windows_clipboard_sequence: Arc<Mutex<u32>>,
    #[cfg(target_os = "macos")]
    last_macos_clipboard_change_count: Arc<Mutex<i64>>,
    #[cfg(target_os = "linux")]
    last_linux_clipboard_timestamp: Arc<Mutex<String>>,
}

impl Clone for ClipboardMonitor {
    fn clone(&self) -> Self {
        ClipboardMonitor {
            clipboard: self.clipboard.clone(),
            last_text: self.last_text.clone(),
            last_image_hash: self.last_image_hash.clone(),
            is_internal_change: self.is_internal_change.clone(),
            suppressed_text_hash: self.suppressed_text_hash.clone(),
            last_processed_image_hash: self.last_processed_image_hash.clone(),
            #[cfg(target_os = "windows")]
            last_windows_clipboard_sequence: self.last_windows_clipboard_sequence.clone(),
            #[cfg(target_os = "macos")]
            last_macos_clipboard_change_count: self.last_macos_clipboard_change_count.clone(),
            #[cfg(target_os = "linux")]
            last_linux_clipboard_timestamp: self.last_linux_clipboard_timestamp.clone(),
        }
    }
}

fn get_hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

impl ClipboardMonitor {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(ClipboardMonitor {
            clipboard: Arc::new(Mutex::new(Clipboard::new()?)),
            last_text: Arc::new(Mutex::new(String::new())),
            last_image_hash: Arc::new(Mutex::new(String::new())),
            is_internal_change: Arc::new(Mutex::new(false)),
            suppressed_text_hash: Arc::new(Mutex::new(None)),
            last_processed_image_hash: Arc::new(Mutex::new(String::new())),
            #[cfg(target_os = "windows")]
            last_windows_clipboard_sequence: Arc::new(Mutex::new(0)),
            #[cfg(target_os = "macos")]
            last_macos_clipboard_change_count: Arc::new(Mutex::new(0)),
            #[cfg(target_os = "linux")]
            last_linux_clipboard_timestamp: Arc::new(Mutex::new(String::new())),
        })
    }

    pub async fn mark_internal_change(&self) {
        let mut guard = self.is_internal_change.lock().await;
        *guard = true;
    }

    pub async fn start_monitoring(
        &self,
        db: Db,
        app_handle: tauri::AppHandle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let clipboard = self.clipboard.clone();
        let last_text = self.last_text.clone();
        let last_image_hash = self.last_image_hash.clone();
        let is_internal_change = self.is_internal_change.clone();
        let suppressed_text_hash = self.suppressed_text_hash.clone();
        let last_processed_image_hash = self.last_processed_image_hash.clone();
        #[cfg(target_os = "windows")]
        let last_windows_clipboard_sequence = self.last_windows_clipboard_sequence.clone();
        #[cfg(target_os = "macos")]
        let last_macos_clipboard_change_count = self.last_macos_clipboard_change_count.clone();
        #[cfg(target_os = "linux")]
        let last_linux_clipboard_timestamp = self.last_linux_clipboard_timestamp.clone();

        let mut ticker = interval(Duration::from_millis(200)); // Check every 200ms for better responsiveness

        loop {
            ticker.tick().await;

            // Check if this is an internal change
            let is_internal = {
                let mut guard = is_internal_change.lock().await;
                let val = *guard;
                *guard = false;
                val
            };

            if is_internal {
                log::debug!("[Clipboard] Skipping internal change");
                #[cfg(target_os = "windows")]
                {
                    let current_sequence = get_windows_clipboard_sequence_number();
                    if current_sequence != 0 {
                        *last_windows_clipboard_sequence.lock().await = current_sequence;
                    }
                }
                #[cfg(target_os = "macos")]
                {
                    *last_macos_clipboard_change_count.lock().await =
                        get_macos_clipboard_change_count();
                }
                continue;
            }

            #[cfg(target_os = "windows")]
            let windows_clipboard_changed = {
                let current_sequence = get_windows_clipboard_sequence_number();
                let mut last_sequence = last_windows_clipboard_sequence.lock().await;

                if current_sequence == 0 {
                    true
                } else if *last_sequence == 0 {
                    *last_sequence = current_sequence;
                    true
                } else if current_sequence == *last_sequence {
                    continue;
                } else {
                    log::debug!(
                        "[Clipboard] Windows clipboard sequence changed: {} -> {}",
                        *last_sequence,
                        current_sequence
                    );
                    *last_sequence = current_sequence;
                    *last_processed_image_hash.lock().await = String::new();
                    *last_image_hash.lock().await = String::new();
                    true
                }
            };

            #[cfg(target_os = "macos")]
            let macos_clipboard_changed = {
                let current_count = get_macos_clipboard_change_count();
                let mut last_count = last_macos_clipboard_change_count.lock().await;
                if *last_count != 0 && *last_count == current_count {
                    continue;
                }
                *last_count = current_count;
                *last_processed_image_hash.lock().await = String::new();
                *last_image_hash.lock().await = String::new();
                true
            };

            // Try to get text first (fast check)
            let mut current_text = {
                let mut clip = clipboard.lock().await;
                read_clipboard_text(&mut clip)
            };

            #[cfg(target_os = "linux")]
            let possible_linux_file_list = looks_like_file_uri_list(&current_text);

            #[cfg(target_os = "linux")]
            let linux_clipboard_timestamp = if current_text.is_empty() || possible_linux_file_list {
                self.get_linux_clipboard_timestamp().await
            } else {
                None
            };

            #[cfg(target_os = "linux")]
            if let Some(timestamp) = linux_clipboard_timestamp.as_deref() {
                let last_timestamp = last_linux_clipboard_timestamp.lock().await.clone();
                if !last_timestamp.is_empty() && last_timestamp == timestamp {
                    continue;
                }
            }

            // Linux fallback for text
            #[cfg(target_os = "linux")]
            let linux_targets = if current_text.is_empty() || possible_linux_file_list {
                self.get_linux_clipboard_targets().await
            } else {
                None
            };

            #[cfg(target_os = "linux")]
            let mut current_file_content = String::new();
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            let mut current_file_content = String::new();
            #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
            let current_file_content = String::new();

            #[cfg(target_os = "linux")]
            if current_text.is_empty() || possible_linux_file_list {
                if let Some(paths) = self
                    .try_get_linux_file_list_with_targets(linux_targets.as_deref())
                    .await
                {
                    current_file_content = paths.join("\n");
                    current_text.clear();
                    log::info!(
                        "[Clipboard] Linux file list detected, files: {}",
                        paths.len()
                    );
                }
            }

            #[cfg(target_os = "windows")]
            if windows_clipboard_changed {
                if let Some(paths) = get_windows_file_list().await {
                    current_file_content = paths.join("\n");
                    current_text.clear();
                    log::info!(
                        "[Clipboard] Windows file list detected, files: {}",
                        paths.len()
                    );
                }
            }

            #[cfg(target_os = "macos")]
            if macos_clipboard_changed {
                if let Some(paths) = get_macos_file_list() {
                    current_file_content = paths.join("\n");
                    current_text.clear();
                    log::info!(
                        "[Clipboard] macOS file list detected, files: {}",
                        paths.len()
                    );
                }
            }

            #[cfg(target_os = "linux")]
            if current_text.is_empty() && current_file_content.is_empty() {
                let should_try_text_fallback = linux_targets
                    .as_deref()
                    .map(|targets| {
                        linux_targets_have_text(targets) && !linux_targets_have_image(targets)
                    })
                    .unwrap_or(false);

                if should_try_text_fallback {
                    if let Some(output) = run_xclip_with_timeout(
                        &["-o", "-selection", "clipboard", "-t", "UTF8_STRING"],
                        300,
                    )
                    .await
                    {
                        if output.status.success() && !output.stdout.is_empty() {
                            let fallback_text = String::from_utf8_lossy(&output.stdout).to_string();
                            log::debug!(
                                "[Clipboard] Linux text fallback used, length: {}",
                                fallback_text.len()
                            );
                            current_text = fallback_text;
                        }
                    }
                } else if linux_targets.is_some() {
                    // Image-only clipboards should not be coerced through xclip's text target.
                } else if let Some(output) =
                    run_xclip_with_timeout(&["-o", "-selection", "clipboard"], 300).await
                {
                    if output.status.success() && !output.stdout.is_empty() {
                        let fallback_text = String::from_utf8_lossy(&output.stdout).to_string();
                        log::debug!(
                            "[Clipboard] Linux text fallback used, length: {}",
                            fallback_text.len()
                        );
                        current_text = fallback_text;
                    }
                }
            }

            // Try to get image (only if no text)
            let mut current_image_data = String::new();
            let mut current_image_hash = String::new();

            #[cfg(target_os = "linux")]
            if current_text.is_empty() && current_file_content.is_empty() {
                if let Some(image_read) = self
                    .try_get_linux_image_with_targets(linux_targets.as_deref())
                    .await
                {
                    current_image_data = image_read.data_url;
                    current_image_hash =
                        format!("{:x}", Sha256::digest(current_image_data.as_bytes()));

                    let last_hash = last_processed_image_hash.lock().await.clone();
                    if last_hash == current_image_hash {
                        if let Some(timestamp) = linux_clipboard_timestamp.as_deref() {
                            *last_linux_clipboard_timestamp.lock().await = timestamp.to_string();
                        }
                        continue;
                    }

                    log::info!(
                        "[Clipboard] Successfully read new image as {}, size: {}, elapsed: {}ms",
                        image_read.mime_type,
                        image_read.bytes_len,
                        image_read.elapsed_ms
                    );

                    let existing_id =
                        ClipboardRepository::check_duplicate_image(&db, &current_image_hash, 10)
                            .await
                            .ok()
                            .flatten();

                    if let Some(existing_id) = existing_id {
                        log::info!(
                            "[Clipboard] Duplicate Linux image found, moving to top: {}",
                            existing_id
                        );
                        if let Err(e) = ClipboardRepository::move_to_top(&db, existing_id).await {
                            log::error!("[Clipboard] Failed to move existing image to top: {}", e);
                        }

                        *last_processed_image_hash.lock().await = current_image_hash.clone();
                        if let Some(timestamp) = linux_clipboard_timestamp.as_deref() {
                            *last_linux_clipboard_timestamp.lock().await = timestamp.to_string();
                        }

                        if let Some(tab_id) = item_tab_id(&db, existing_id).await {
                            emit_clipboard_changed(
                                &app_handle,
                                vec![tab_id],
                                vec![existing_id],
                                "duplicate-image",
                            );
                        }
                        continue;
                    }
                }
            }

            if current_image_data.is_empty()
                && current_file_content.is_empty()
                && (current_text.is_empty() || cfg!(not(target_os = "linux")))
            {
                // Get raw image data from clipboard
                let image_info = {
                    let mut clip = clipboard.lock().await;
                    read_clipboard_image(&mut clip)
                };

                if let Some((width, height, bytes)) = image_info {
                    if !current_text.is_empty() {
                        log::debug!(
                            "[Clipboard] Clipboard contains image and text; prioritizing image"
                        );
                        current_text.clear();
                    }

                    // Calculate hash for fast dedup (on raw RGBA bytes)
                    current_image_hash = calculate_image_hash(&bytes, width as u32, height as u32);

                    // Early exit: check if we already processed this exact image recently
                    let last_hash = last_processed_image_hash.lock().await.clone();
                    let should_skip_processed_image = {
                        #[cfg(target_os = "windows")]
                        {
                            last_hash == current_image_hash && !windows_clipboard_changed
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            last_hash == current_image_hash
                        }
                    };

                    if should_skip_processed_image {
                        // Already processed this image, skip entire processing cycle
                        // log::debug!("[Clipboard] Image unchanged, skipping processing cycle");
                        #[cfg(target_os = "linux")]
                        if let Some(timestamp) = linux_clipboard_timestamp.as_deref() {
                            *last_linux_clipboard_timestamp.lock().await = timestamp.to_string();
                        }
                        continue;
                    }

                    log::debug!(
                        "[Clipboard] Image detected: {}x{}, bytes: {}",
                        width,
                        height,
                        bytes.len()
                    );

                    // Check if this image already exists in database (fast hash check)
                    let hash_clone = current_image_hash.clone();
                    let existing_id =
                        ClipboardRepository::check_duplicate_image(&db, &hash_clone, 10)
                            .await
                            .ok()
                            .flatten();

                    if let Some(existing_id) = existing_id {
                        // Duplicate found, move to top
                        log::info!(
                            "[Clipboard] Duplicate image found, moving to top: {}",
                            existing_id
                        );
                        if let Err(e) = ClipboardRepository::move_to_top(&db, existing_id).await {
                            log::error!("[Clipboard] Failed to move existing image to top: {}", e);
                        }

                        // Mark as processed to avoid re-processing
                        *last_processed_image_hash.lock().await = current_image_hash.clone();
                        #[cfg(target_os = "linux")]
                        if let Some(timestamp) = linux_clipboard_timestamp.as_deref() {
                            *last_linux_clipboard_timestamp.lock().await = timestamp.to_string();
                        }

                        if let Some(tab_id) = item_tab_id(&db, existing_id).await {
                            emit_clipboard_changed(
                                &app_handle,
                                vec![tab_id],
                                vec![existing_id],
                                "duplicate-image",
                            );
                        }
                        continue; // Skip to next iteration
                    }

                    // Convert RGBA to PNG in blocking thread (CPU intensive)
                    let bytes_clone = bytes.clone();
                    let encode_start = Instant::now();
                    let png_result = tokio::task::spawn_blocking(move || {
                        rgba_to_png(&bytes_clone, width as u32, height as u32)
                    })
                    .await;

                    match png_result {
                        Ok(Ok(png_bytes)) => {
                            current_image_data = format!(
                                "data:image/png;base64,{}",
                                general_purpose::STANDARD.encode(&png_bytes)
                            );
                            log::info!(
                                "[Clipboard] Image converted to PNG, size: {}, elapsed: {}ms",
                                png_bytes.len(),
                                encode_start.elapsed().as_millis()
                            );
                        }
                        Ok(Err(e)) => {
                            log::error!("[Clipboard] Failed to convert image to PNG: {}", e);
                        }
                        Err(e) => {
                            log::error!("[Clipboard] PNG conversion task failed: {}", e);
                        }
                    }
                }
            }

            let mut changed_tab_ids: Vec<i64> = Vec::new();
            let mut changed_item_ids: Vec<i64> = Vec::new();

            // Handle file list change
            if !current_file_content.is_empty() {
                let file_hash = format!("{:x}", Sha256::digest(current_file_content.as_bytes()));
                let last = last_text.lock().await.clone();

                if current_file_content != last {
                    let metadata = self.get_metadata().await;
                    let item = ClipboardItemInput {
                        item_type: "file".to_string(),
                        content: current_file_content.clone(),
                        content_hash: Some(file_hash),
                        metadata: Some(serde_json::to_string(&metadata).unwrap_or_default()),
                        tags: Some("[]".to_string()),
                        tab_id: None,
                        is_sensitive: Some(0),
                        is_pinned: Some(0),
                    };

                    match ClipboardRepository::create_for_auto_capture_tabs(&db, item).await {
                        Ok(ids) => {
                            if !ids.is_empty() {
                                *last_text.lock().await = current_file_content.clone();
                                changed_item_ids.extend(ids.iter().copied());
                                for id in ids {
                                    if let Some(tab_id) = item_tab_id(&db, id).await {
                                        changed_tab_ids.push(tab_id);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("[Clipboard] Failed to save file list: {}", e);
                        }
                    }
                }

                #[cfg(target_os = "linux")]
                if let Some(timestamp) = linux_clipboard_timestamp.as_deref() {
                    *last_linux_clipboard_timestamp.lock().await = timestamp.to_string();
                }
            }

            // Handle text change
            if current_file_content.is_empty() && !current_text.is_empty() {
                let last = last_text.lock().await.clone();
                if current_text != last {
                    log::debug!(
                        "[Clipboard] Text change detected, length: {}",
                        current_text.len()
                    );

                    // Calculate hash for dedup
                    let text_hash = format!("{:x}", Sha256::digest(current_text.as_bytes()));

                    let should_suppress = {
                        let mut suppressed = suppressed_text_hash.lock().await;
                        if suppressed.as_deref() == Some(text_hash.as_str()) {
                            *suppressed = None;
                            true
                        } else {
                            false
                        }
                    };

                    if should_suppress {
                        log::debug!("[Clipboard] Suppressing internally copied text");
                        *last_text.lock().await = current_text.clone();
                        *last_image_hash.lock().await = String::new();
                        *last_processed_image_hash.lock().await = String::new();
                        #[cfg(target_os = "linux")]
                        if let Some(timestamp) = linux_clipboard_timestamp.as_deref() {
                            *last_linux_clipboard_timestamp.lock().await = timestamp.to_string();
                        }
                        continue;
                    }

                    // Check if this text already exists (fast hash check)
                    let existing_id = ClipboardRepository::check_duplicate_text(&db, &text_hash)
                        .await
                        .ok()
                        .flatten();

                    if let Some(existing_id) = existing_id {
                        // Duplicate found from an external clipboard write: move it to top and notify the UI.
                        log::info!(
                            "[Clipboard] Duplicate text found, moving to top: {}",
                            existing_id
                        );
                        *last_text.lock().await = current_text.clone();
                        if let Err(e) = ClipboardRepository::move_to_top(&db, existing_id).await {
                            log::error!("[Clipboard] Failed to move existing text to top: {}", e);
                        }
                        if let Some(tab_id) = item_tab_id(&db, existing_id).await {
                            emit_clipboard_changed(
                                &app_handle,
                                vec![tab_id],
                                vec![existing_id],
                                "duplicate-text",
                            );
                        }
                        continue; // Skip to next iteration
                    }

                    // Not a duplicate - update last_text and create new item
                    *last_text.lock().await = current_text.clone();
                    // Also update last_processed_image_hash to avoid re-processing if clipboard content doesn't change
                    *last_image_hash.lock().await = String::new();
                    *last_processed_image_hash.lock().await = String::new(); // Clear image hash when text changes

                    let metadata = self.get_metadata().await;
                    let is_sensitive = SENSITIVE_REGEX.is_match(&current_text);

                    let item = ClipboardItemInput {
                        item_type: "text".to_string(),
                        content: current_text,
                        content_hash: Some(text_hash),
                        metadata: Some(serde_json::to_string(&metadata).unwrap_or_default()),
                        tags: Some("[]".to_string()),
                        tab_id: None,
                        is_sensitive: Some(if is_sensitive { 1 } else { 0 }),
                        is_pinned: Some(0),
                    };

                    match ClipboardRepository::create_for_auto_capture_tabs(&db, item).await {
                        Ok(ids) => {
                            if !ids.is_empty() {
                                log::info!(
                                    "[Clipboard] Text items saved to {} tabs, IDs: {:?}",
                                    ids.len(),
                                    ids
                                );
                                changed_item_ids.extend(ids.iter().copied());
                                for id in ids {
                                    if let Some(tab_id) = item_tab_id(&db, id).await {
                                        changed_tab_ids.push(tab_id);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("[Clipboard] Failed to save text: {}", e);
                        }
                    }
                }
            }

            // Handle image change (already checked for duplicates above)
            if !current_image_data.is_empty() && !current_image_hash.is_empty() {
                let last = last_image_hash.lock().await.clone();
                let should_save_image = {
                    #[cfg(target_os = "windows")]
                    {
                        current_image_hash != last || windows_clipboard_changed
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        current_image_hash != last
                    }
                };

                if should_save_image {
                    log::debug!("[Clipboard] New image detected, saving...");
                    *last_image_hash.lock().await = current_image_hash.clone();

                    // Mark as processed to avoid re-processing in duplicate check
                    *last_processed_image_hash.lock().await = current_image_hash.clone();
                    #[cfg(target_os = "linux")]
                    if let Some(timestamp) = linux_clipboard_timestamp.as_deref() {
                        *last_linux_clipboard_timestamp.lock().await = timestamp.to_string();
                    }

                    // Create new item (already checked for duplicates)
                    let metadata = self.get_metadata().await;
                    let hash_for_db = current_image_hash.clone();

                    let item = ClipboardItemInput {
                        item_type: "image".to_string(),
                        content: current_image_data,
                        content_hash: Some(hash_for_db),
                        metadata: Some(serde_json::to_string(&metadata).unwrap_or_default()),
                        tags: Some("[]".to_string()),
                        tab_id: None,
                        is_sensitive: Some(0),
                        is_pinned: Some(0),
                    };

                    match ClipboardRepository::create_for_auto_capture_tabs(&db, item).await {
                        Ok(ids) => {
                            if !ids.is_empty() {
                                log::info!(
                                    "[Clipboard] Image items saved to {} tabs, IDs: {:?}",
                                    ids.len(),
                                    ids
                                );
                                changed_item_ids.extend(ids.iter().copied());
                                for id in ids {
                                    if let Some(tab_id) = item_tab_id(&db, id).await {
                                        changed_tab_ids.push(tab_id);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("[Clipboard] Failed to save image: {}", e);
                        }
                    }
                }
            }

            // Notify frontend of change immediately for better responsiveness
            if !changed_tab_ids.is_empty() {
                log::info!("[Clipboard] Notifying frontend of clipboard change");
                changed_tab_ids.sort_unstable();
                changed_tab_ids.dedup();
                emit_clipboard_changed(&app_handle, changed_tab_ids, changed_item_ids, "create");
            }
        }
    }

    #[cfg(target_os = "linux")]
    async fn get_linux_clipboard_targets(&self) -> Option<String> {
        let output =
            run_xclip_with_timeout(&["-o", "-selection", "clipboard", "-t", "TARGETS"], 300)
                .await?;

        if !output.status.success() {
            return None;
        }

        let targets = String::from_utf8_lossy(&output.stdout).to_string();
        Some(targets)
    }

    #[cfg(target_os = "linux")]
    async fn get_linux_clipboard_timestamp(&self) -> Option<String> {
        let output =
            run_xclip_with_timeout(&["-o", "-selection", "clipboard", "-t", "TIMESTAMP"], 150)
                .await?;

        if !output.status.success() || output.stdout.is_empty() {
            return None;
        }

        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    #[cfg(target_os = "linux")]
    async fn try_get_linux_file_list_with_targets(
        &self,
        targets: Option<&str>,
    ) -> Option<Vec<String>> {
        let owned_targets;
        let targets = match targets {
            Some(targets) => targets,
            None => {
                owned_targets = self.get_linux_clipboard_targets().await?;
                &owned_targets
            }
        };

        if !targets
            .lines()
            .any(|target| target.trim() == "text/uri-list")
        {
            return None;
        }

        let output = run_xclip_with_timeout(
            &["-o", "-selection", "clipboard", "-t", "text/uri-list"],
            500,
        )
        .await?;

        if !output.status.success() || output.stdout.is_empty() {
            return None;
        }

        let uri_list = String::from_utf8_lossy(&output.stdout);
        let paths = paths_from_uri_list(&uri_list);
        if paths.is_empty() {
            None
        } else {
            Some(paths)
        }
    }

    #[cfg(target_os = "linux")]
    async fn try_get_linux_image_with_targets(
        &self,
        targets: Option<&str>,
    ) -> Option<LinuxImageRead> {
        let owned_targets;
        let targets = match targets {
            Some(targets) => targets,
            None => {
                owned_targets = match self.get_linux_clipboard_targets().await {
                    Some(targets) => targets,
                    None => return None,
                };
                &owned_targets
            }
        };

        if !linux_targets_have_image(targets) {
            return None;
        }

        let formats = [
            ("image/png", "png"),
            ("image/jpeg", "jpeg"),
            ("image/bmp", "bmp"),
            ("image/x-ico", "x-icon"),
            ("image/webp", "webp"),
        ];

        for (mime_type, ext) in formats {
            if !targets.lines().any(|target| target.trim() == mime_type) {
                continue;
            }

            let read_start = Instant::now();
            if let Some(output) =
                run_xclip_with_timeout(&["-o", "-selection", "clipboard", "-t", mime_type], 800)
                    .await
            {
                if output.status.success() && !output.stdout.is_empty() {
                    return Some(LinuxImageRead {
                        data_url: format!(
                            "data:image/{};base64,{}",
                            ext,
                            general_purpose::STANDARD.encode(&output.stdout)
                        ),
                        mime_type,
                        bytes_len: output.stdout.len(),
                        elapsed_ms: read_start.elapsed().as_millis(),
                    });
                }
            }
        }

        None
    }

    async fn get_metadata(&self) -> Metadata {
        let mut window_title = "Unknown".to_string();
        let mut source_app = "Unknown".to_string();

        #[cfg(target_os = "linux")]
        {
            if let Ok(output) = Command::new("sh")
                .arg("-c")
                .arg("xprop -id $(xprop -root _NET_ACTIVE_WINDOW | cut -d \" \" -f 5) WM_NAME WM_CLASS")
                .output()
            {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);

                    // Parse WM_NAME
                    if let Some(captures) = regex::Regex::new(r#"WM_NAME\((?:STRING|UTF8_STRING)\) = "(.*)""#)
                        .unwrap()
                        .captures(&stdout)
                    {
                        window_title = captures.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                    }

                    // Parse WM_CLASS
                    if let Some(captures) = regex::Regex::new(r#"WM_CLASS\((?:STRING|UTF8_STRING)\) = (.*)"#)
                        .unwrap()
                        .captures(&stdout)
                    {
                        let classes = captures.get(1).map(|m| m.as_str()).unwrap_or_default();
                        let parts: Vec<&str> = classes.split(", ").collect();
                        source_app = parts.last().map(|s| s.replace("\"", "")).unwrap_or_default();
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Ok(active_window) = active_win_pos_rs::get_active_window() {
                window_title = active_window.title;
                source_app = active_window.app_name;
            }
        }

        #[cfg(target_os = "macos")]
        {
            // Use AppleScript to get the frontmost application and window title
            let applescript = r#"
                tell application "System Events"
                    set frontApp to first application process whose frontmost is true
                    set appName to name of frontApp
                    set windowTitle to ""
                    try
                        set windowList to windows of frontApp
                        if (count of windowList) > 0 then
                            set frontWindow to item 1 of windowList
                            set windowTitle to name of frontWindow
                        end if
                    end try
                    return appName & "|" & windowTitle
                end tell
            "#;

            if let Ok(output) = Command::new("osascript")
                .arg("-e")
                .arg(applescript)
                .output()
            {
                if output.status.success() {
                    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let parts: Vec<&str> = result.splitn(2, '|').collect();
                    if parts.len() == 2 {
                        source_app = parts[0].to_string();
                        window_title = parts[1].to_string();
                        if window_title.is_empty() {
                            window_title = "Unknown".to_string();
                        }
                    }
                    log::debug!(
                        "[Clipboard] macOS metadata - app: {}, title: {}",
                        source_app,
                        window_title
                    );
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    log::warn!("[Clipboard] Failed to get macOS metadata: {}", stderr);
                }
            } else {
                log::warn!("[Clipboard] Failed to execute osascript for macOS metadata");
            }
        }

        Metadata {
            source: std::env::consts::OS.to_string(),
            source_app,
            window_title,
            source_host: get_hostname(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub async fn write_text(&self, text: &str) -> Result<(), Box<dyn std::error::Error>> {
        log::info!(
            "[Clipboard] write_text called with content length: {}",
            text.len()
        );

        let text_hash = format!("{:x}", Sha256::digest(text.as_bytes()));
        *self.suppressed_text_hash.lock().await = Some(text_hash.clone());

        // Mark as internal change BEFORE writing to prevent monitoring loop from catching it
        self.mark_internal_change().await;

        // On Linux, try xclip first as it's more reliable
        if cfg!(target_os = "linux") {
            use std::io::Write;
            use std::process::Stdio;

            match Command::new("xclip")
                .arg("-selection")
                .arg("clipboard")
                .arg("-in")
                .stdin(Stdio::piped())
                .spawn()
            {
                Ok(mut child) => {
                    if let Some(mut stdin) = child.stdin.take() {
                        if let Err(e) = stdin.write_all(text.as_bytes()) {
                            log::error!("[Clipboard] Failed to write to xclip stdin: {}", e);
                            self.clear_suppressed_text_hash(&text_hash).await;
                            return Err(e.into());
                        }
                    }
                    match child.wait() {
                        Ok(status) => {
                            if status.success() {
                                log::info!("[Clipboard] Text written to clipboard using xclip");
                                self.finish_internal_text_write(text, &text_hash).await;
                                // Give monitoring loop time to see the internal_change flag
                                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                                return Ok(());
                            } else {
                                log::warn!("[Clipboard] xclip exited with status: {}", status);
                                // Fall through to try arboard
                            }
                        }
                        Err(e) => {
                            log::warn!("[Clipboard] Failed to wait for xclip: {}", e);
                            // Fall through to try arboard
                        }
                    }
                }
                Err(e) => {
                    log::warn!("[Clipboard] Failed to spawn xclip: {}", e);
                    // Fall through to try arboard
                }
            }
        }

        // Try arboard as backup (or primary on non-Linux)
        let mut clip = self.clipboard.lock().await;
        match clip.set_text(text.to_string()) {
            Ok(_) => {
                log::info!("[Clipboard] Text written to clipboard using arboard");
                drop(clip);
                self.finish_internal_text_write(text, &text_hash).await;
                // Give monitoring loop time to see the internal_change flag
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                Ok(())
            }
            Err(e) => {
                log::error!("[Clipboard] Failed to write text to clipboard: {}", e);
                self.clear_suppressed_text_hash(&text_hash).await;
                Err(e.into())
            }
        }
    }

    async fn finish_internal_text_write(&self, text: &str, text_hash: &str) {
        *self.last_text.lock().await = text.to_string();
        *self.last_processed_image_hash.lock().await = String::new();
        self.clear_suppressed_text_hash(text_hash).await;
    }

    async fn clear_suppressed_text_hash(&self, text_hash: &str) {
        let mut suppressed = self.suppressed_text_hash.lock().await;
        if suppressed.as_deref() == Some(text_hash) {
            *suppressed = None;
        }
    }

    pub async fn write_image(&self, data_url: &str) -> Result<(), Box<dyn std::error::Error>> {
        log::debug!(
            "[Clipboard] write_image called with data_url length: {}",
            data_url.len()
        );

        // Mark as internal change BEFORE writing to prevent monitoring loop from catching it
        self.mark_internal_change().await;

        // Parse data URL
        let parts: Vec<&str> = data_url.split(',').collect();
        if parts.len() != 2 {
            return Err("Invalid data URL format".into());
        }

        let base64_data = parts[1];
        let png_bytes = general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| format!("Failed to decode base64: {}", e))?;

        log::debug!("[Clipboard] Decoded {} bytes from base64", png_bytes.len());

        // Decode PNG to RGBA
        let img = image::load_from_memory(&png_bytes)
            .map_err(|e| format!("Failed to decode PNG: {}", e))?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();

        log::debug!("[Clipboard] Image decoded: {}x{}", width, height);

        let mut clip = self.clipboard.lock().await;

        match clip.set_image(arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: rgba.into_raw().into(),
        }) {
            Ok(_) => {
                log::info!("[Clipboard] Image written to clipboard successfully");
                drop(clip);
                // Give monitoring loop time to see the internal_change flag
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                Ok(())
            }
            Err(e) => {
                log::error!("[Clipboard] Failed to write image: {}", e);
                Err(e.into())
            }
        }
    }

    pub async fn write_files(&self, content: &str) -> Result<(), Box<dyn std::error::Error>> {
        let paths = parse_file_list(content);
        if paths.is_empty() {
            return Err("No existing files to copy".into());
        }

        log::info!(
            "[Clipboard] write_files called with {} file(s)",
            paths.len()
        );
        self.mark_internal_change().await;

        #[cfg(target_os = "windows")]
        {
            let quoted_paths = paths
                .iter()
                .map(|path| format!("'{}'", path.display().to_string().replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",");
            let script = format!("Set-Clipboard -LiteralPath @({})", quoted_paths);
            let status = Command::new("powershell")
                .arg("-NoProfile")
                .arg("-Command")
                .arg(script)
                .status()?;
            if status.success() {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                return Ok(());
            }
            return Err(format!("PowerShell Set-Clipboard failed: {}", status).into());
        }

        #[cfg(target_os = "macos")]
        {
            let file_refs = paths
                .iter()
                .map(|path| {
                    format!(
                        "POSIX file \"{}\"",
                        path.display()
                            .to_string()
                            .replace('\\', "\\\\")
                            .replace('"', "\\\"")
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let script = format!("set the clipboard to {{{}}}", file_refs);
            let status = Command::new("osascript").arg("-e").arg(script).status()?;
            if status.success() {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                return Ok(());
            }
            Err(format!("osascript clipboard file write failed: {}", status).into())
        }

        #[cfg(target_os = "linux")]
        {
            use std::io::Write;
            use std::process::Stdio;

            let uri_list = paths
                .iter()
                .map(|path| file_uri_from_path(path.as_path()))
                .collect::<Vec<_>>()
                .join("\n");
            let mut child = Command::new("xclip")
                .arg("-selection")
                .arg("clipboard")
                .arg("-t")
                .arg("text/uri-list")
                .arg("-in")
                .stdin(Stdio::piped())
                .spawn()?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(uri_list.as_bytes())?;
            }

            let status = child.wait()?;
            if status.success() {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                Ok(())
            } else {
                Err(format!("xclip file write failed: {}", status).into())
            }
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Err("File clipboard is not supported on this platform".into())
        }
    }

    pub async fn move_to_top(&self, db: &Db, id: i64) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("[Clipboard] move_to_top called for id: {}", id);

        // Get current timestamp
        let now = chrono::Utc::now();

        // Update the item's updated_at to move it to top (non-pinned items are sorted by updated_at DESC)
        match ClipboardRepository::update_timestamp(db, id, now).await {
            Ok(_) => {
                log::info!("[Clipboard] Item moved to top");
                Ok(())
            }
            Err(e) => {
                log::error!("[Clipboard] Failed to move item to top: {}", e);
                Err(Box::new(e))
            }
        }
    }
}
