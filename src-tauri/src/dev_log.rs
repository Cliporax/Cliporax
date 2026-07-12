//! Development-mode log file backend
//! Enabled only in debug_assertions mode; writes frontend and backend logs to files
//! Supports JSON format, daily rotation, trace ID tracking, and async batched writes

use crate::async_log_writer::{AsyncBatchWriter, BatchWriterConfig};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::sync::Arc;
use std::sync::Mutex;

/// Log entry structure passed from the frontend
#[derive(Debug, Deserialize)]
pub struct DevLogEntry {
    pub level: String,
    pub component: String,
    pub message: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub timestamp: Option<String>,
}

/// JSON log entry structure written to files
#[derive(Debug, Serialize)]
struct JsonLogEntry {
    ts: String,
    source: String,
    target: String,
    window: String,
    component: String,
    level: String,
    message: String,
    trace_id: Option<String>,
    span_id: Option<String>,
    parent_span_id: Option<String>,
    event_type: Option<String>,
    seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<serde_json::Value>,
}

/// Log file backend
#[cfg(debug_assertions)]
pub struct DevLogFileBackend {
    #[allow(dead_code)]
    file: Arc<Mutex<File>>,
    async_writer: Option<Arc<Mutex<AsyncBatchWriter>>>,
    sequence_counter: Arc<Mutex<u64>>,
}

/// Production-mode stub
#[cfg(not(debug_assertions))]
pub struct DevLogFileBackend {
    _dummy: (),
}

#[cfg(debug_assertions)]
impl DevLogFileBackend {
    /// Initialize log files with daily rotation
    pub fn init(app_handle: &tauri::AppHandle) -> Result<Self, String> {
        let app_data = crate::portable::app_data_dir(app_handle)?;

        let logs_dir = app_data.join("logs");
        std::fs::create_dir_all(&logs_dir)
            .map_err(|e| format!("Failed to create logs dir: {}", e))?;

        // Generate the file name by date
        let today = chrono::Local::now().format("%Y-%m-%d");
        let log_path = logs_dir.join(format!("dev-{}.log", today));

        // Append mode without overwriting
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("Failed to open log file: {}", e))?;

        log::info!("[DevLog] Log file initialized at: {:?}", log_path);

        // Clean up logs older than 7 days
        Self::cleanup_old_logs(&logs_dir);

        let file_arc = Arc::new(Mutex::new(file));

        // Create the async batch writer
        let async_writer = Arc::new(Mutex::new(AsyncBatchWriter::new(
            file_arc.clone(),
            BatchWriterConfig::default(),
        )));

        Ok(Self {
            file: file_arc,
            async_writer: Some(async_writer),
            sequence_counter: Arc::new(Mutex::new(0)),
        })
    }

    /// Write a log entry in JSON format
    pub fn write(&self, source: &str, level: &str, component: &str, message: &str) {
        self.write_with_trace(source, level, component, message, None, None, None)
    }

    /// Write a log entry with a trace ID
    #[allow(clippy::too_many_arguments)]
    pub fn write_with_trace(
        &self,
        source: &str,
        level: &str,
        component: &str,
        message: &str,
        trace_id: Option<String>,
        span_id: Option<String>,
        event_type: Option<String>,
    ) {
        self.write_with_full_trace(
            source, level, component, message, trace_id, span_id, None, // parent_span_id
            event_type, None, // context
        )
    }

    /// Write a log entry with full trace information
    #[allow(clippy::too_many_arguments)]
    pub fn write_with_full_trace(
        &self,
        source: &str,
        level: &str,
        component: &str,
        message: &str,
        trace_id: Option<String>,
        span_id: Option<String>,
        parent_span_id: Option<String>,
        event_type: Option<String>,
        context: Option<serde_json::Value>,
    ) {
        // Increment the sequence number
        let seq = {
            let mut counter = self.sequence_counter.lock().unwrap();
            *counter += 1;
            *counter
        };

        let entry = JsonLogEntry {
            ts: chrono::Utc::now().to_rfc3339(),
            source: source.to_string(),
            target: "ALL".to_string(),  // Default value; adjust if needed
            window: "main".to_string(), // Default value; adjust if needed
            component: component.to_string(),
            level: level.to_string(),
            message: message.to_string(),
            trace_id,
            span_id,
            parent_span_id,
            event_type,
            seq,
            context,
        };

        // Serialize to JSON
        if let Ok(json_line) = serde_json::to_string(&entry) {
            // Use async batched writes
            if let Some(writer) = &self.async_writer {
                if let Ok(w) = writer.lock() {
                    w.write(json_line);
                }
            }
        }
    }

    /// Clean up logs older than 7 days
    fn cleanup_old_logs(logs_dir: &std::path::Path) {
        if let Ok(entries) = std::fs::read_dir(logs_dir) {
            let now = chrono::Local::now();

            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    // Match the dev-YYYY-MM-DD.log format
                    if name.starts_with("dev-") && name.ends_with(".log") {
                        let date_str = &name[4..14]; // Extract YYYY-MM-DD
                        if let Ok(file_date) =
                            chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                        {
                            let file_datetime =
                                chrono::DateTime::<chrono::Local>::from_naive_utc_and_offset(
                                    file_date.and_hms_opt(0, 0, 0).unwrap(),
                                    *chrono::Local::now().offset(),
                                );

                            let age = now.signed_duration_since(file_datetime);
                            if age.num_days() > 7 {
                                let _ = std::fs::remove_file(entry.path());
                                log::debug!("[DevLog] Cleaned up old log: {}", name);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(not(debug_assertions))]
impl DevLogFileBackend {
    /// Production-mode initialization without creating files
    pub fn init(_app_handle: &tauri::AppHandle) -> Result<Self, String> {
        Ok(Self { _dummy: () })
    }

    /// No-op in production mode
    pub fn write(&self, _source: &str, _level: &str, _component: &str, _message: &str) {}

    /// No-op in production mode with trace
    pub fn write_with_trace(
        &self,
        _source: &str,
        _level: &str,
        _component: &str,
        _message: &str,
        _trace_id: Option<String>,
        _span_id: Option<String>,
        _event_type: Option<String>,
    ) {
    }
}

/// Custom logger that captures backend logs and writes them to files
#[cfg(debug_assertions)]
pub struct DualLogger {
    backend: std::sync::Arc<DevLogFileBackend>,
}

#[cfg(debug_assertions)]
impl log::Log for DualLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info || metadata.target().starts_with("cliporax")
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // Get the current trace context
        let trace_id = crate::trace_context::get_trace_id();
        let span_id = crate::trace_context::get_span_id();

        // Format the message with trace data
        let message = crate::trace_context::format_with_trace(&record.args().to_string());

        // Output to the console
        eprintln!("[{}] {}", record.level(), message);

        // Write to the file in JSON format with trace data
        self.backend.write_with_trace(
            "BACKEND",
            &record.level().to_string(),
            record.target(),
            &message,
            trace_id,
            span_id,
            None,
        );
    }

    fn flush(&self) {}
}

/// Install the custom logger in development mode only
#[cfg(debug_assertions)]
pub fn install_logger(backend: std::sync::Arc<DevLogFileBackend>) -> Result<(), String> {
    use std::sync::OnceLock;

    static LOGGER: OnceLock<DualLogger> = OnceLock::new();

    let logger = DualLogger { backend };
    let logger = LOGGER.get_or_init(|| logger);

    log::set_logger(logger)
        .map(|()| log::set_max_level(log::LevelFilter::Debug))
        .map_err(|e| format!("Failed to set logger: {}", e))
}

/// Empty implementation in production mode
#[cfg(not(debug_assertions))]
pub fn install_logger(_backend: std::sync::Arc<DevLogFileBackend>) -> Result<(), String> {
    Ok(())
}
