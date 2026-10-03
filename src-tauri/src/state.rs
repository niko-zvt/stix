use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone)]
pub struct LastSavedNote {
    pub path: String,
    pub folder: String,
}

pub struct ViewingNoteContent {
    pub id: String,
    pub content: String,
    pub folder: String,
    pub path: String,
}

pub struct AppState {
    pub shortcut_to_folder: Mutex<HashMap<String, String>>,
    pub shortcut_to_action: Mutex<HashMap<String, String>>,
    pub viewing_notes: Mutex<HashMap<String, ViewingNoteContent>>,
    pub previous_focused_window: Mutex<Option<String>>,
    pub postit_was_visible: Mutex<bool>,
    pub last_saved_note: Mutex<Option<LastSavedNote>>,
    /// Notes opened or saved this session, oldest first. A deleted latest note
    /// falls back to the previous entry.
    pub recent_notes: Mutex<Vec<LastSavedNote>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            shortcut_to_folder: Mutex::new(HashMap::new()),
            shortcut_to_action: Mutex::new(HashMap::new()),
            viewing_notes: Mutex::new(HashMap::new()),
            previous_focused_window: Mutex::new(None),
            postit_was_visible: Mutex::new(false),
            last_saved_note: Mutex::new(None),
            recent_notes: Mutex::new(Vec::new()),
        }
    }

    pub fn remember_note(&self, path: &str, folder: &str) {
        if path.trim().is_empty() {
            return;
        }
        let note = LastSavedNote {
            path: path.to_string(),
            folder: folder.to_string(),
        };
        let mut last = self
            .last_saved_note
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *last = Some(note.clone());
        drop(last);

        let mut recent = self
            .recent_notes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        recent.retain(|existing| existing.path != note.path);
        recent.push(note);
        let extra = recent.len().saturating_sub(64);
        if extra > 0 {
            recent.drain(0..extra);
        }
    }

    pub fn forget_note(&self, path: &str) {
        if path.is_empty() {
            return;
        }
        let mut last = self
            .last_saved_note
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if last.as_ref().is_some_and(|note| note.path == path) {
            *last = None;
        }
        drop(last);
        self.recent_notes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|note| note.path != path);
    }
}
