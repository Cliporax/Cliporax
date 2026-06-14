//! Preview window commands

use tauri::{Manager, WebviewWindowBuilder};

const MAX_IMAGE_DATA_BYTES: usize = 50 * 1024 * 1024;
const PREVIEW_WINDOW_LABEL: &str = "preview-image";

static PREVIEW_WINDOW_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

    if let Some(window) = app.get_webview_window(PREVIEW_WINDOW_LABEL) {
        update_preview_window(&window, &title, &image_data)?;
        if let Err(e) = window.set_focus() {
            log::warn!("[Command] Failed to focus preview window: {}", e);
        }
        log::info!("[Command] Preview window updated: {}", PREVIEW_WINDOW_LABEL);
        return Ok(PREVIEW_WINDOW_LABEL.to_string());
    }

    let blank_url = "about:blank"
        .parse()
        .map_err(|e| format!("Invalid preview URL: {}", e))?;

    match WebviewWindowBuilder::new(
        &app,
        PREVIEW_WINDOW_LABEL,
        tauri::WebviewUrl::External(blank_url),
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
        Ok(window) => {
            if let Err(e) = update_preview_window(&window, &title, &image_data) {
                let _ = window.close();
                return Err(e);
            }

            log::info!("[Command] Preview window created: {}", PREVIEW_WINDOW_LABEL);
            Ok(PREVIEW_WINDOW_LABEL.to_string())
        }
        Err(e) => {
            log::error!("[Command] Failed to create preview window: {}", e);
            Err(e.to_string())
        }
    }
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

fn update_preview_window(
    window: &tauri::WebviewWindow,
    title: &str,
    image_data: &str,
) -> Result<(), String> {
    window.set_title(title).map_err(|e| {
        log::error!("[Command] Failed to set preview title: {}", e);
        format!("Failed to set preview title: {}", e)
    })?;

    let html_content = generate_preview_html(title, image_data);
    let html_json = serde_json::to_string(&html_content)
        .map_err(|e| format!("Failed to encode preview HTML: {}", e))?;
    let script = format!(
        "document.open(); document.write({}); document.close();",
        html_json
    );

    window.eval(&script).map_err(|e| {
        log::error!("[Command] Failed to initialize preview window: {}", e);
        format!("Failed to initialize preview window: {}", e)
    })
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

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Generate HTML content for the preview window with embedded image data
fn generate_preview_html(title: &str, image_data: &str) -> String {
    let escaped_title = escape_html(title);
    let image_data_json = serde_json::to_string(image_data).unwrap_or_else(|_| "\"\"".to_string());

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title}</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}

        body {{
            display: flex;
            flex-direction: column;
            height: 100vh;
            overflow: hidden;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background-color: #15161b;
        }}

        .image-container {{
            flex: 1;
            display: flex;
            align-items: center;
            justify-content: center;
            overflow: hidden;
            position: relative;
            background: #15161b;
        }}

        .preview-image {{
            max-width: 100%;
            max-height: 100%;
            object-fit: contain;
            transition: transform 0.2s ease;
        }}

        .zoom-controls {{
            position: fixed;
            bottom: 16px;
            left: 50%;
            transform: translateX(-50%);
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 10px 14px;
            border-radius: 8px;
            background: rgba(38, 42, 50, 0.96);
            border: 1px solid rgba(255,255,255,0.12);
            box-shadow: 0 4px 20px rgba(0,0,0,0.3);
        }}

        .zoom-controls button {{
            min-width: 36px;
            height: 36px;
            padding: 0 10px;
            border: none;
            border-radius: 6px;
            cursor: pointer;
            display: flex;
            align-items: center;
            justify-content: center;
            background: transparent;
            color: #e2e8f0;
            transition: background 0.15s ease;
            font-size: 14px;
        }}

        .zoom-controls button:hover {{
            background: rgba(255,255,255,0.15);
        }}

        .zoom-level {{
            min-width: 50px;
            text-align: center;
            font-size: 13px;
            font-weight: 500;
            color: #a0aec0;
        }}

        .divider {{
            width: 1px;
            height: 24px;
            margin: 0 4px;
            background: rgba(255,255,255,0.15);
        }}

        .info-text {{
            position: fixed;
            top: 10px;
            left: 50%;
            transform: translateX(-50%);
            padding: 6px 12px;
            background: rgba(38, 42, 50, 0.92);
            border-radius: 6px;
            font-size: 12px;
            color: #a0aec0;
            opacity: 0.85;
        }}
    </style>
</head>
<body>
    <div class="info-text">Mouse wheel zoom | Esc close | +/- zoom | 0 reset</div>

    <div class="image-container" id="imageContainer">
        <img class="preview-image" id="previewImage" alt="Clipboard image preview" />
    </div>

    <div class="zoom-controls">
        <button onclick="zoomOut()" title="Zoom out (-)">-</button>
        <span class="zoom-level" id="zoomLevel">100%</span>
        <button onclick="zoomIn()" title="Zoom in (+)">+</button>
        <div class="divider"></div>
        <button onclick="resetZoom()" title="Reset (0)">0</button>
        <button onclick="downloadImage()" title="Download">Save</button>
    </div>

    <script>
        let zoomLevel = 100;
        const imageData = {image_data_json};
        const img = document.getElementById('previewImage');
        const zoomLevelEl = document.getElementById('zoomLevel');
        const imageContainer = document.getElementById('imageContainer');
        img.src = imageData;

        function zoomIn() {{
            zoomLevel = Math.min(zoomLevel + 25, 500);
            applyZoom();
        }}

        function zoomOut() {{
            zoomLevel = Math.max(zoomLevel - 25, 25);
            applyZoom();
        }}

        function resetZoom() {{
            zoomLevel = 100;
            applyZoom();
        }}

        function applyZoom() {{
            img.style.transform = `scale(${{zoomLevel / 100}})`;
            zoomLevelEl.textContent = `${{zoomLevel}}%`;
        }}

        window.zoomIn = zoomIn;
        window.zoomOut = zoomOut;
        window.resetZoom = resetZoom;

        function downloadImage() {{
            const link = document.createElement('a');
            link.download = 'clipboard-image.png';
            link.href = imageData;
            link.click();
        }}
        window.downloadImage = downloadImage;

        imageContainer.addEventListener('wheel', (event) => {{
            event.preventDefault();
            if (event.deltaY < 0) zoomIn();
            else zoomOut();
        }}, {{ passive: false }});

        document.addEventListener('keydown', (event) => {{
            if (event.key === 'Escape') window.close();
            if (event.key === '+' || event.key === '=') zoomIn();
            if (event.key === '-') zoomOut();
            if (event.key === '0') resetZoom();
        }});
    </script>
</body>
</html>"#,
        title = escaped_title,
        image_data_json = image_data_json
    )
}
