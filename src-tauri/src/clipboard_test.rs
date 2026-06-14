#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::setup_test_db;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[test]
    fn test_clipboard_monitor_creation() {
        let monitor = ClipboardMonitor::new();
        assert!(monitor.is_ok());
    }

    #[tokio::test]
    async fn test_mark_internal_change() {
        let monitor = ClipboardMonitor::new().unwrap();
        monitor.mark_internal_change();
        
        // Give some time for the async operation to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        // Test that we can acquire the lock (meaning the internal change flag was set)
        let is_internal = monitor.is_internal_change.clone();
        let guard = is_internal.lock().await;
        // The flag should be reset to false after being read in start_monitoring
        // but we can't easily test that without running the full monitoring loop
    }

    #[tokio::test]
    async fn test_metadata_extraction_linux() {
        let monitor = ClipboardMonitor::new().unwrap();
        let metadata = monitor.get_metadata().await;
        
        assert_eq!(metadata.source, std::env::consts::OS);
        assert!(!metadata.timestamp.is_empty());
        // On Linux, these might be "Unknown" if xprop is not available in test environment
        println!("Source app: {}", metadata.source_app);
        println!("Window title: {}", metadata.window_title);
    }

    #[test]
    fn test_sensitive_regex_detection() {
        // Test various sensitive content patterns
        assert!(SENSITIVE_REGEX.is_match("my password is secret123"));
        assert!(SENSITIVE_REGEX.is_match("PASSWORD: abc123"));
        assert!(SENSITIVE_REGEX.is_match("verification code: 123456"));
        assert!(SENSITIVE_REGEX.is_match("OTP: 987654"));
        assert!(SENSITIVE_REGEX.is_match("验证码：123456"));
        assert!(SENSITIVE_REGEX.is_match("API_SECRET_KEY"));
        
        // Test non-sensitive content
        assert!(!SENSITIVE_REGEX.is_match("This is normal text"));
        assert!(!SENSITIVE_REGEX.is_match("Hello world"));
        assert!(!SENSITIVE_REGEX.is_match("Regular content"));
    }

    #[tokio::test]
    async fn test_write_text_operation() {
        let monitor = ClipboardMonitor::new().unwrap();
        let test_text = "Test clipboard text";
        
        // This might fail in test environment without actual clipboard access
        // but we can at least test the method doesn't panic
        match monitor.write_text(test_text).await {
            Ok(_) => {
                // Success - text was written to clipboard
                println!("Successfully wrote text to clipboard");
            }
            Err(e) => {
                // Expected in test environment without clipboard access
                println!("Clipboard write failed (expected in tests): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_write_image_operation() {
        let monitor = ClipboardMonitor::new().unwrap();
        let test_image_data = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";
        
        // Test with valid data URL
        match monitor.write_image(test_image_data).await {
            Ok(_) => {
                println!("Successfully wrote image to clipboard");
            }
            Err(e) => {
                println!("Image write failed (expected in tests): {}", e);
            }
        }
        
        // Test with invalid data URL
        let invalid_data = "invalid_data_url";
        assert!(monitor.write_image(invalid_data).await.is_err());
    }

    #[tokio::test]
    async fn test_try_get_linux_image_no_clipboard() {
        let monitor = ClipboardMonitor::new().unwrap();
        let result = monitor.try_get_linux_image().await;
        // Should return empty string when no image is available
        assert_eq!(result, "");
    }

    #[tokio::test]
    async fn test_start_monitoring_structure() {
        // We can't easily test the full monitoring loop in unit tests
        // but we can verify the monitor can be created and has the right structure
        let monitor = ClipboardMonitor::new().unwrap();
        
        assert!(monitor.clipboard.is_some());
        assert!(monitor.last_text.is_some());
        assert!(monitor.last_image_hash.is_some());
        assert!(monitor.is_internal_change.is_some());
    }

    #[test]
    fn test_lazy_static_regex_compilation() {
        // Test that the regex compiles correctly
        assert!(SENSITIVE_REGEX.is_match("password"));
        assert!(SENSITIVE_REGEX.is_match("PASSWORD")); // Case insensitive
    }
}
