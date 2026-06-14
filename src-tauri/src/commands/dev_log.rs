//! Dev log command - handles frontend log entries from IPC

use crate::dev_log::{DevLogEntry, DevLogFileBackend};
use crate::trace_context;

/// IPC command for handling frontend log write requests
#[tauri::command]
pub fn dev_log_write(
    entry: DevLogEntry,
    _trace_id: Option<String>,
    _span_id: Option<String>,
    backend: tauri::State<'_, std::sync::Arc<DevLogFileBackend>>,
) {
    // Extract trace information passed from the frontend
    let trace_id = entry.trace_id.or(_trace_id);
    let span_id = entry.span_id.or(_span_id);

    // Set the current trace context
    trace_context::extract_trace_context(&trace_id, &span_id, &None, &None);

    // Write the log with trace data
    backend.write_with_trace(
        "FRONTEND",
        &entry.level,
        &entry.component,
        &entry.message,
        trace_id,
        span_id,
        None,
    );

    // Clear the trace context
    trace_context::clear_trace_context();
}
