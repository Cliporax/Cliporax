//! Async Batch Log Writer
//!
//! Provides asynchronous batch writing for log entries to improve performance.
//! Uses mpsc channel + background thread to batch flush logs.

use std::fs::File;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Log entry to be written
pub struct LogEntry {
    pub json_line: String,
}

/// Async batch writer configuration
pub struct BatchWriterConfig {
    /// Maximum number of entries before flush
    pub batch_size: usize,
    /// Maximum time to wait before flush (milliseconds)
    pub flush_interval_ms: u64,
}

impl Default for BatchWriterConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            flush_interval_ms: 100,
        }
    }
}

/// Async batch log writer
pub struct AsyncBatchWriter {
    sender: Option<Sender<LogEntry>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl AsyncBatchWriter {
    /// Create a new async batch writer
    pub fn new(file: Arc<Mutex<File>>, config: BatchWriterConfig) -> Self {
        let (tx, rx): (Sender<LogEntry>, Receiver<LogEntry>) = mpsc::channel();

        let handle = thread::spawn(move || {
            Self::background_worker(file, rx, config);
        });

        Self {
            sender: Some(tx),
            handle: Some(handle),
        }
    }

    /// Send a log entry to be written
    pub fn write(&self, json_line: String) {
        if let Some(tx) = &self.sender {
            // Non-blocking send, drop if channel is full
            // Use send() since try_send() is not available in stable Rust
            let _ = tx.send(LogEntry { json_line });
        }
    }

    /// Flush and shutdown the writer
    pub fn shutdown(&mut self) {
        // Drop sender to signal worker to exit
        self.sender.take();

        // Wait for worker to finish
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    /// Background worker thread
    fn background_worker(
        file: Arc<Mutex<File>>,
        receiver: Receiver<LogEntry>,
        config: BatchWriterConfig,
    ) {
        let mut buffer = Vec::with_capacity(config.batch_size);
        let flush_interval = Duration::from_millis(config.flush_interval_ms);

        loop {
            // Try to receive with timeout
            match receiver.recv_timeout(flush_interval) {
                Ok(entry) => {
                    buffer.push(entry.json_line);

                    // Collect more entries if available
                    while buffer.len() < config.batch_size {
                        match receiver.try_recv() {
                            Ok(entry) => buffer.push(entry.json_line),
                            Err(_) => break, // No more entries
                        }
                    }

                    // Flush buffer
                    Self::flush_to_file(&file, &mut buffer);
                }
                Err(_) => {
                    // Timeout reached, flush if buffer has entries
                    if !buffer.is_empty() {
                        Self::flush_to_file(&file, &mut buffer);
                    }
                }
            }

            // Check if sender is dropped (channel closed)
            if receiver.try_recv().is_err() && buffer.is_empty() {
                // Channel closed and buffer empty, exit
                break;
            }
        }

        // Final flush
        if !buffer.is_empty() {
            Self::flush_to_file(&file, &mut buffer);
        }
    }

    /// Flush buffer to file
    fn flush_to_file(file: &Arc<Mutex<File>>, buffer: &mut Vec<String>) {
        if let Ok(mut f) = file.lock() {
            use std::io::Write;
            for line in buffer.drain(..) {
                let _ = writeln!(f, "{}", line);
            }
            let _ = f.flush();
        }
    }
}

impl Drop for AsyncBatchWriter {
    fn drop(&mut self) {
        self.shutdown();
    }
}
