//! System readiness and native utility commands.

use base64::{engine::general_purpose, Engine as _};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[tauri::command]
pub async fn app_ready() -> Result<bool, String> {
    Ok(true)
}

#[tauri::command]
pub async fn qrscanner_capture_region(window: tauri::Window) -> Result<String, String> {
    log::info!("[QRScanner] INFO: Starting region capture");

    let was_visible = window.is_visible().unwrap_or(false);
    if was_visible {
        if let Err(e) = window.hide() {
            log::warn!("[QRScanner] WARN: Failed to hide main window: {}", e);
        }
        std::thread::sleep(Duration::from_millis(120));
    }

    let capture_result = tauri::async_runtime::spawn_blocking(capture_region_png)
        .await
        .map_err(|e| format!("Region capture task failed: {}", e))?;

    if was_visible {
        if let Err(e) = window.show() {
            log::warn!("[QRScanner] WARN: Failed to restore main window: {}", e);
        }

        let window_clone = window.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Err(e) = window_clone.set_focus() {
                log::warn!("[QRScanner] WARN: Failed to focus main window: {}", e);
            }
        });
    }

    let png = capture_result?;
    if png.is_empty() {
        return Err("No image was captured".to_string());
    }

    Ok(format!(
        "data:image/png;base64,{}",
        general_purpose::STANDARD.encode(png)
    ))
}

fn capture_region_png() -> Result<Vec<u8>, String> {
    #[cfg(target_os = "linux")]
    {
        capture_region_png_linux()
    }

    #[cfg(target_os = "macos")]
    {
        capture_region_png_macos()
    }

    #[cfg(target_os = "windows")]
    {
        capture_region_png_windows()
    }
}

#[cfg(target_os = "linux")]
fn capture_region_png_linux() -> Result<Vec<u8>, String> {
    let path = temp_capture_path("qrscanner-region", "png");
    match Command::new("scrot").arg("-s").arg(&path).status() {
        Ok(status) if status.success() => {
            log::info!("[QRScanner] INFO: Region captured with scrot");
            return read_and_remove_capture(path);
        }
        Ok(_) => {
            let _ = std::fs::remove_file(&path);
            return Err("Region capture was cancelled or failed".to_string());
        }
        Err(e) => {
            log::warn!("[QRScanner] WARN: scrot unavailable: {}", e);
        }
    }

    match Command::new("import").arg("png:-").output() {
        Ok(output) if output.status.success() && !output.stdout.is_empty() => {
            log::info!("[QRScanner] INFO: Region captured with ImageMagick import");
            Ok(output.stdout)
        }
        Ok(output) => Err(format!(
            "Region capture was cancelled or failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )),
        Err(e) => Err(format!(
            "Failed to start region capture. Install ImageMagick import or scrot: {}",
            e
        )),
    }
}

#[cfg(target_os = "macos")]
fn capture_region_png_macos() -> Result<Vec<u8>, String> {
    let path = temp_capture_path("qrscanner-region", "png");
    let status = Command::new("screencapture")
        .arg("-i")
        .arg("-x")
        .arg(&path)
        .status()
        .map_err(|e| format!("Failed to start macOS region capture: {}", e))?;

    if !status.success() {
        let _ = std::fs::remove_file(&path);
        return Err("Region capture was cancelled or failed".to_string());
    }

    read_and_remove_capture(path)
}

#[cfg(target_os = "windows")]
fn capture_region_png_windows() -> Result<Vec<u8>, String> {
    let image_path = temp_capture_path("qrscanner-region", "png");
    let script_path = temp_capture_path("qrscanner-region-capture", "ps1");

    let script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class CliporaxNativeCapture {{
    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();
}}
"@

[CliporaxNativeCapture]::SetProcessDPIAware() | Out-Null

$outputPath = {output_path}
$bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
$form = New-Object System.Windows.Forms.Form
$form.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::None
$form.StartPosition = [System.Windows.Forms.FormStartPosition]::Manual
$form.Bounds = $bounds
$form.TopMost = $true
$form.ShowInTaskbar = $false
$form.Cursor = [System.Windows.Forms.Cursors]::Cross
$form.BackColor = [System.Drawing.Color]::Black
$form.Opacity = 0.18
$form.KeyPreview = $true
$form.Add_Shown({{ $form.Activate() }})

$state = [PSCustomObject]@{{
    Selecting = $false
    Start = [System.Drawing.Point]::Empty
    Current = [System.Drawing.Point]::Empty
}}

function Get-SelectionRectangle {{
    $x = [Math]::Min($state.Start.X, $state.Current.X)
    $y = [Math]::Min($state.Start.Y, $state.Current.Y)
    $w = [Math]::Abs($state.Start.X - $state.Current.X)
    $h = [Math]::Abs($state.Start.Y - $state.Current.Y)
    return New-Object System.Drawing.Rectangle -ArgumentList $x, $y, $w, $h
}}

$form.Add_KeyDown({{
    if ($_.KeyCode -eq [System.Windows.Forms.Keys]::Escape) {{
        $form.DialogResult = [System.Windows.Forms.DialogResult]::Cancel
        $form.Close()
    }}
}})

$form.Add_MouseDown({{
    if ($_.Button -ne [System.Windows.Forms.MouseButtons]::Left) {{ return }}
    $state.Selecting = $true
    $state.Start = New-Object System.Drawing.Point -ArgumentList $_.X, $_.Y
    $state.Current = $state.Start
    $form.Invalidate()
}})

$form.Add_MouseMove({{
    if (-not $state.Selecting) {{ return }}
    $state.Current = New-Object System.Drawing.Point -ArgumentList $_.X, $_.Y
    $form.Invalidate()
}})

$form.Add_MouseUp({{
    if (-not $state.Selecting -or $_.Button -ne [System.Windows.Forms.MouseButtons]::Left) {{ return }}
    $state.Selecting = $false
    $state.Current = New-Object System.Drawing.Point -ArgumentList $_.X, $_.Y
    $rect = Get-SelectionRectangle

    if ($rect.Width -lt 4 -or $rect.Height -lt 4) {{
        $form.DialogResult = [System.Windows.Forms.DialogResult]::Cancel
        $form.Close()
        return
    }}

    $screenRect = New-Object System.Drawing.Rectangle -ArgumentList @(
        ($bounds.Left + $rect.Left),
        ($bounds.Top + $rect.Top),
        $rect.Width,
        $rect.Height
    )
    $bitmap = New-Object System.Drawing.Bitmap -ArgumentList $screenRect.Width, $screenRect.Height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)

    try {{
        $form.Hide()
        Start-Sleep -Milliseconds 120
        $graphics.CopyFromScreen($screenRect.Location, [System.Drawing.Point]::Empty, $screenRect.Size)
        $bitmap.Save($outputPath, [System.Drawing.Imaging.ImageFormat]::Png)
        $form.DialogResult = [System.Windows.Forms.DialogResult]::OK
    }} finally {{
        $graphics.Dispose()
        $bitmap.Dispose()
        $form.Close()
    }}
}})

$form.Add_Paint({{
    if (-not $state.Selecting) {{ return }}
    $rect = Get-SelectionRectangle
    if ($rect.Width -le 0 -or $rect.Height -le 0) {{ return }}

    $pen = New-Object System.Drawing.Pen([System.Drawing.Color]::DodgerBlue, 2)
    $brush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(48, 30, 144, 255))

    try {{
        $_.Graphics.FillRectangle($brush, $rect)
        $_.Graphics.DrawRectangle($pen, $rect)
    }} finally {{
        $brush.Dispose()
        $pen.Dispose()
    }}
}})

$result = $form.ShowDialog()
$form.Dispose()

if ($result -ne [System.Windows.Forms.DialogResult]::OK) {{
    exit 2
}}
"#,
        output_path = powershell_single_quoted_path(&image_path)
    );

    std::fs::write(&script_path, script)
        .map_err(|e| format!("Failed to prepare Windows region capture: {}", e))?;

    let status = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-STA")
        .arg("-WindowStyle")
        .arg("Hidden")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script_path)
        .creation_flags(0x08000000)
        .status()
        .map_err(|e| format!("Failed to start Windows region capture: {}", e))?;

    if let Err(e) = std::fs::remove_file(&script_path) {
        log::warn!(
            "[QRScanner] WARN: Failed to remove temporary script {:?}: {}",
            script_path,
            e
        );
    }

    if !status.success() {
        let _ = std::fs::remove_file(&image_path);
        return Err("Region capture was cancelled or failed".to_string());
    }

    log::info!("[QRScanner] INFO: Region captured with Windows overlay");
    read_and_remove_capture(image_path)
}

#[cfg(target_os = "windows")]
fn powershell_single_quoted_path(path: &std::path::Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn temp_capture_path(prefix: &str, extension: &str) -> std::path::PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    std::env::temp_dir().join(format!(
        "{}-{}-{}.{}",
        prefix,
        std::process::id(),
        millis,
        extension
    ))
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn read_and_remove_capture(path: std::path::PathBuf) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read capture: {}", e))?;
    if let Err(e) = std::fs::remove_file(&path) {
        log::warn!(
            "[QRScanner] WARN: Failed to remove temporary capture {:?}: {}",
            path,
            e
        );
    }
    Ok(bytes)
}
