use std::process::Command;

/// Check if the app has Accessibility permissions on macOS
#[cfg(target_os = "macos")]
pub fn check_accessibility_permissions() -> bool {
    // Use AppleScript to test if we can access System Events
    let applescript = r#"
        tell application "System Events"
            return name of first application process whose frontmost is true
        end tell
    "#;
    
    let output = Command::new("osascript")
        .arg("-e")
        .arg(applescript)
        .output();
    
    match output {
        Ok(result) => result.status.success(),
        Err(_) => false,
    }
}

/// Request Accessibility permissions on macOS by opening System Preferences
#[cfg(target_os = "macos")]
pub fn request_accessibility_permissions() {
    log::info!("[Accessibility] Requesting Accessibility permissions");
    
    // Open System Preferences to the Accessibility section
    let _ = Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .output();
}

/// Show a native macOS alert dialog
#[cfg(target_os = "macos")]
pub fn show_permission_alert(title: &str, message: &str) {
    let applescript = format!(
        r#"
        tell application "System Events"
            activate
            display dialog "{}" with title "{}" buttons {{"Open Settings", "Cancel"}} default button "Open Settings" with icon caution
            if button returned of result is "Open Settings" then
                do shell script "open 'x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility'"
            end if
        end tell
        "#,
        message, title
    );
    
    let _ = Command::new("osascript")
        .arg("-e")
        .arg(&applescript)
        .output();
}

/// Check and request Accessibility permissions on app startup
#[cfg(target_os = "macos")]
pub fn ensure_accessibility_permissions() -> bool {
    if check_accessibility_permissions() {
        log::info!("[Accessibility] Permissions granted");
        true
    } else {
        log::warn!("[Accessibility] Permissions not granted, showing prompt");
        show_permission_alert(
            "Accessibility Permission Required",
            "Cliporax needs Accessibility permission to simulate paste (Cmd+V) and restore window focus.\\n\\nPlease:\\n1. Click 'Open Settings'\\n2. Unlock the padlock\\n3. Check 'cliporax' in the list\\n\\nThen try again."
        );
        false
    }
}
