//! Trace Context for Backend
//!
//! Extracts and manages trace context from IPC calls.
//! Provides thread-local storage for trace_id and span_id.

use std::cell::RefCell;

/// Trace context extracted from IPC calls
#[derive(Debug, Clone, Default)]
pub struct BackendTraceContext {
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub sequence: Option<i64>,
}

thread_local! {
    /// Thread-local storage for current trace context
    static CURRENT_TRACE: RefCell<BackendTraceContext> = RefCell::new(BackendTraceContext::default());
}

/// Extract trace context from IPC arguments
///
/// This function should be called at the beginning of each Tauri command
/// to extract trace information injected by the frontend.
///
/// # Example
/// ```rust,ignore
/// #[tauri::command]
/// async fn settings_update(
///     state: State<'_, Arc<AppState>>,
///     new_settings: AppSettings,
///     _trace_id: Option<String>,
///     _span_id: Option<String>,
/// ) -> Result<(), String> {
///     // Extract and set trace context
///     extract_trace_context(&_trace_id, &_span_id, &None, &None);
///     
///     // Now all log messages in this function will include trace info
///     log::info!("settings_update called");
///     
///     // ... business logic
///     
///     Ok(())
/// }
/// ```
pub fn extract_trace_context(
    trace_id: &Option<String>,
    span_id: &Option<String>,
    parent_span_id: &Option<String>,
    sequence: &Option<i64>,
) {
    CURRENT_TRACE.with(|cell| {
        let mut ctx = cell.borrow_mut();
        ctx.trace_id = trace_id.clone();
        ctx.span_id = span_id.clone();
        ctx.parent_span_id = parent_span_id.clone();
        ctx.sequence = *sequence;
    });
}

/// Get current trace context
pub fn get_trace_context() -> BackendTraceContext {
    CURRENT_TRACE.with(|cell| cell.borrow().clone())
}

/// Get current trace_id
pub fn get_trace_id() -> Option<String> {
    CURRENT_TRACE.with(|cell| cell.borrow().trace_id.clone())
}

/// Get current span_id
pub fn get_span_id() -> Option<String> {
    CURRENT_TRACE.with(|cell| cell.borrow().span_id.clone())
}

/// Clear current trace context (call when command completes)
pub fn clear_trace_context() {
    CURRENT_TRACE.with(|cell| {
        *cell.borrow_mut() = BackendTraceContext::default();
    });
}

/// Format log message with trace context
///
/// Returns formatted string like: `[TRACE:abc123] [SPAN:s1] message`
/// or just `message` if no trace context is active.
pub fn format_with_trace(message: &str) -> String {
    let ctx = get_trace_context();

    if let (Some(trace_id), Some(span_id)) = (&ctx.trace_id, &ctx.span_id) {
        format!("[TRACE:{}] [SPAN:{}] {}", trace_id, span_id, message)
    } else {
        message.to_string()
    }
}

/// RAII guard that automatically clears trace context when dropped
pub struct TraceGuard {
    _private: (),
}

impl TraceGuard {
    /// Create a new trace guard
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for TraceGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TraceGuard {
    fn drop(&mut self) {
        clear_trace_context();
    }
}
