use std::sync::atomic::{AtomicBool, Ordering};

// Global static variables for window state control
// These variables are shared across the app to control auto-hide behavior
/// Flag to temporarily disable auto-hide when shortcut is triggered
pub static SHORTCUT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Flag to temporarily disable auto-hide when paste operation is in progress
pub static PASTE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Flag to permanently disable auto-hide when window is pinned
/// This is controlled by user via the pin button
pub static WINDOW_PINNED: AtomicBool = AtomicBool::new(false);

/// Flag to track if window is being dragged (for Windows compatibility)
pub static WINDOW_DRAGGING: AtomicBool = AtomicBool::new(false);

/// Flag to track if window is being resized (to prevent auto-hide during resize)
pub static WINDOW_RESIZING: AtomicBool = AtomicBool::new(false);

/// Flag to track if a context menu is open (to prevent auto-hide during right-click)
pub static CONTEXT_MENU_OPEN: AtomicBool = AtomicBool::new(false);

/// Centralized window state management structure
/// Can be managed as Tauri state for better type safety
#[derive(Default)]
pub struct WindowState {
    pub shortcut_in_progress: AtomicBool,
    pub paste_in_progress: AtomicBool,
    pub pinned: AtomicBool,
    pub dragging: AtomicBool,
    pub resizing: AtomicBool,
    pub context_menu_open: AtomicBool,
}

impl WindowState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn should_prevent_auto_hide(&self) -> bool {
        self.shortcut_in_progress.load(Ordering::SeqCst)
            || self.paste_in_progress.load(Ordering::SeqCst)
            || self.pinned.load(Ordering::SeqCst)
            || self.dragging.load(Ordering::SeqCst)
            || self.resizing.load(Ordering::SeqCst)
            || self.context_menu_open.load(Ordering::SeqCst)
    }

    pub fn set_shortcut_in_progress(&self, value: bool) {
        self.shortcut_in_progress.store(value, Ordering::SeqCst);
        log::debug!("[WindowState] shortcut_in_progress = {}", value);
    }

    pub fn set_paste_in_progress(&self, value: bool) {
        self.paste_in_progress.store(value, Ordering::SeqCst);
        log::debug!("[WindowState] paste_in_progress = {}", value);
    }

    pub fn set_pinned(&self, value: bool) {
        self.pinned.store(value, Ordering::SeqCst);
        log::info!("[WindowState] pinned = {}", value);
    }

    pub fn set_dragging(&self, value: bool) {
        self.dragging.store(value, Ordering::SeqCst);
        log::trace!("[WindowState] dragging = {}", value);
    }

    pub fn set_resizing(&self, value: bool) {
        self.resizing.store(value, Ordering::SeqCst);
        log::trace!("[WindowState] resizing = {}", value);
    }

    pub fn set_context_menu_open(&self, value: bool) {
        self.context_menu_open.store(value, Ordering::SeqCst);
        log::debug!("[WindowState] context_menu_open = {}", value);
    }
}

impl std::fmt::Debug for WindowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowState")
            .field(
                "shortcut_in_progress",
                &self.shortcut_in_progress.load(Ordering::SeqCst),
            )
            .field(
                "paste_in_progress",
                &self.paste_in_progress.load(Ordering::SeqCst),
            )
            .field("pinned", &self.pinned.load(Ordering::SeqCst))
            .field("dragging", &self.dragging.load(Ordering::SeqCst))
            .field("resizing", &self.resizing.load(Ordering::SeqCst))
            .field(
                "context_menu_open",
                &self.context_menu_open.load(Ordering::SeqCst),
            )
            .finish()
    }
}
