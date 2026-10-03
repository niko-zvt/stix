use crate::commands::{note_geometry, notes, settings, sticked_notes};
use crate::state::AppState;
use base64::Engine;
use note_geometry::ScreenFrame;
use sticked_notes::StickedNote;
use tauri::{
    AppHandle, Emitter, Listener, Manager, PhysicalPosition, TitleBarStyle, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

const SETTINGS_WINDOW_WIDTH: f64 = 860.0;
const SETTINGS_WINDOW_HEIGHT: f64 = 720.0;
const SETTINGS_WINDOW_MIN_WIDTH: f64 = 760.0;
const SETTINGS_WINDOW_MIN_HEIGHT: f64 = 560.0;

const EDITOR_WINDOW_WIDTH: f64 = 1100.0;
const EDITOR_WINDOW_HEIGHT: f64 = 740.0;
const EDITOR_WINDOW_MIN_WIDTH: f64 = 820.0;
const EDITOR_WINDOW_MIN_HEIGHT: f64 = 520.0;

/// Minimum overlap (in physical pixels) between window and monitor for the position to be usable.
const MIN_OVERLAP: f64 = 80.0;

/// Check if a window at (x, y) with the given size overlaps sufficiently with any connected
/// monitor. All coordinates are in **physical pixels** (same space as `outerPosition()`).
/// Uses rectangle intersection — handles negative coordinates from left/top monitors.
fn is_window_visible_on_any_monitor(app: &AppHandle, x: f64, y: f64, w: f64, h: f64) -> bool {
    let monitors = app
        .get_webview_window("postit")
        .and_then(|win| win.available_monitors().ok());

    let Some(monitors) = monitors else {
        return false;
    };

    for monitor in monitors {
        let pos = monitor.position();
        let size = monitor.size();

        // All values in physical pixels — no scale conversion needed.
        let mx = pos.x as f64;
        let my = pos.y as f64;
        let mw = size.width as f64;
        let mh = size.height as f64;

        // Rectangle intersection: overlap width/height between window and monitor
        let overlap_w = (x + w).min(mx + mw) - x.max(mx);
        let overlap_h = (y + h).min(my + mh) - y.max(my);

        if overlap_w >= MIN_OVERLAP && overlap_h >= MIN_OVERLAP {
            return true;
        }
    }

    false
}

fn remember_last_note(state: &AppState, path: &str, folder: &str) {
    state.remember_note(path, folder);
}

pub fn show_postit_with_folder(app: &AppHandle, folder: &str) {
    if let Some(window) = app.get_webview_window("postit") {
        if let Ok(s) = settings::load_settings_from_file() {
            // Restore persisted capture window size
            let (w, h) = s.capture_window_size.unwrap_or((400.0, 280.0));
            if s.capture_window_size.is_some() {
                let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(w, h)));
            }
            // Restore position only if it's visible on a connected monitor.
            if let Some((x, y)) = s.viewing_window_position {
                if is_window_visible_on_any_monitor(app, x, y, w, h) {
                    let _ = window.set_position(tauri::Position::Physical(PhysicalPosition::new(
                        x as i32, y as i32,
                    )));
                } else {
                    let _ = window.center();
                }
            }
        }
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("shortcut-triggered", folder);
    }
}

/// One new capture window per call. The shared post-it is a single window, so
/// repeated tray clicks would otherwise keep focusing that same note.
/// `anchor` is the tray click, in physical pixels, so the stack starts on that monitor.
pub fn open_fresh_capture(app: &AppHandle, anchor: Option<(f64, f64)>) -> Result<(), String> {
    let settings = settings::get_settings().unwrap_or_default();
    let (width, height) = settings.capture_window_size.unwrap_or((400.0, 280.0));
    let (frame_x, frame_y, frame_w, frame_h, scale) = monitor_for_anchor(app, anchor);
    let slot = next_stack_slot(frame_x, frame_y, frame_w, frame_h);
    let (x, y) = stack_origin(
        slot,
        (frame_x, frame_y, frame_w, frame_h, scale),
        (width, height),
    );
    let label = format!("capture-{}", uuid::Uuid::new_v4());

    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App("index.html?window=postit".into()),
    )
    .title("Stix")
    .inner_size(width, height)
    .min_inner_size(320.0, 200.0)
    .max_inner_size(800.0, 600.0)
    .resizable(true)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()
    .map_err(|error| format!("Failed to open a new note: {error}"))?;
    remember_capture(&label);
    // The builder owns the window. AppKit rejects setLevel off the main thread,
    // and a plain tray click creates this note from the `stix-tray` worker.
    drop(window);

    let app_for_work = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        let Some(window) = app_for_work.get_webview_window(&label) else {
            return;
        };
        let _ = window.set_position(tauri::Position::Physical(PhysicalPosition::new(
            x as i32, y as i32,
        )));
        let _ = window.show();
        let _ = place_sticker(&window, false);
        let _ = window.set_focus();
    }) {
        eprintln!("Failed to show the new note: {error}");
    }
    Ok(())
}

pub fn show_command_palette(app: &AppHandle) {
    {
        let state = app.state::<AppState>();
        let mut postit_visible = state
            .postit_was_visible
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *postit_visible = app
            .get_webview_window("postit")
            .map(|w| w.is_visible().unwrap_or(false))
            .unwrap_or(false);
    }

    for (label, window) in app.webview_windows() {
        if label.starts_with("sticked-") {
            let _ = window.set_always_on_top(false);
        }
    }

    if let Some(window) = app.get_webview_window("command-palette") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let window = WebviewWindowBuilder::new(
        app,
        "command-palette",
        WebviewUrl::App("index.html?window=command-palette".into()),
    )
    .title("Command Palette")
    .inner_size(700.0, 480.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .center()
    .build();

    if let Ok(win) = window {
        let app_handle = app.clone();
        win.on_window_event(move |event| match event {
            tauri::WindowEvent::Focused(focused) => {
                if !focused {
                    for (label, window) in app_handle.webview_windows() {
                        if label.starts_with("sticked-") {
                            let _ = window.set_always_on_top(true);
                        }
                    }
                }
            }
            tauri::WindowEvent::Destroyed => {
                for (label, window) in app_handle.webview_windows() {
                    if label.starts_with("sticked-") {
                        let _ = window.set_always_on_top(true);
                    }
                }

                let state = app_handle.state::<AppState>();
                let postit_visible = *state
                    .postit_was_visible
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());

                if postit_visible {
                    let has_viewing_windows = app_handle
                        .webview_windows()
                        .iter()
                        .any(|(label, _)| label.starts_with("sticked-view-"));
                    if !has_viewing_windows {
                        if let Some(postit) = app_handle.get_webview_window("postit") {
                            let _ = postit.show();
                            let _ = postit.set_focus();
                        }
                    }
                }
            }
            _ => {}
        });
    }
}

pub fn show_settings(app: &AppHandle) {
    {
        let state = app.state::<AppState>();
        let mut prev_window = state
            .previous_focused_window
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *prev_window = None;

        for (label, window) in app.webview_windows() {
            if label.starts_with("sticked-") && window.is_focused().unwrap_or(false) {
                *prev_window = Some(label.clone());
                break;
            }
        }

        let mut postit_visible = state
            .postit_was_visible
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *postit_visible = app
            .get_webview_window("postit")
            .map(|w| w.is_visible().unwrap_or(false))
            .unwrap_or(false);
    }

    for (label, window) in app.webview_windows() {
        if label.starts_with("sticked-") {
            let _ = window.set_always_on_top(false);
        }
    }

    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let window = WebviewWindowBuilder::new(
        app,
        "settings",
        WebviewUrl::App("index.html?window=settings".into()),
    )
    .title("Settings")
    .inner_size(SETTINGS_WINDOW_WIDTH, SETTINGS_WINDOW_HEIGHT)
    .min_inner_size(SETTINGS_WINDOW_MIN_WIDTH, SETTINGS_WINDOW_MIN_HEIGHT)
    .resizable(true)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .center()
    .build();

    if let Ok(win) = window {
        let app_handle = app.clone();
        win.on_window_event(move |event| {
            if let tauri::WindowEvent::Destroyed = event {
                for (label, window) in app_handle.webview_windows() {
                    if label.starts_with("sticked-") {
                        let _ = window.set_always_on_top(true);
                    }
                }

                let state = app_handle.state::<AppState>();
                let prev_window = state
                    .previous_focused_window
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let postit_visible = *state
                    .postit_was_visible
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());

                if let Some(label) = prev_window.as_ref() {
                    if let Some(window) = app_handle.get_webview_window(label) {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                } else if postit_visible {
                    if let Some(postit) = app_handle.get_webview_window("postit") {
                        let _ = postit.show();
                        let _ = postit.set_focus();
                    }
                }
            }
        });
    }
}

/// Stix's full editor mode: a real, decorated, taskbar-visible app window
/// (unlike the transparent always-on-top sticky windows). Singleton — if it
/// already exists we just focus it.
pub fn show_editor(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("editor") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let _ = WebviewWindowBuilder::new(
        app,
        "editor",
        WebviewUrl::App("index.html?window=editor".into()),
    )
    .title("Stix Editor")
    .inner_size(EDITOR_WINDOW_WIDTH, EDITOR_WINDOW_HEIGHT)
    .min_inner_size(EDITOR_WINDOW_MIN_WIDTH, EDITOR_WINDOW_MIN_HEIGHT)
    .resizable(true)
    .title_bar_style(TitleBarStyle::Overlay)
    .hidden_title(true)
    .center()
    .build();
}

#[tauri::command]
pub fn open_editor(app: AppHandle) -> Result<bool, String> {
    show_editor(&app);
    Ok(true)
}

#[tauri::command]
pub fn hide_window(window: tauri::Window) {
    let _ = window.hide();
}

#[tauri::command]
pub fn hide_postit(app: AppHandle) {
    if let Some(window) = app.get_webview_window("postit") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn create_sticked_window(app: AppHandle, note: StickedNote) -> Result<bool, String> {
    let window_label = format!("sticked-{}", note.id);

    if app.get_webview_window(&window_label).is_some() {
        return Ok(true);
    }

    let saved_position = note.position;
    let (width, height) = note.size.unwrap_or((400.0, 280.0));
    let url = format!("index.html?window=sticked&id={}", note.id);

    // Build hidden — position after creation using PhysicalPosition to avoid
    // the logical/physical mismatch in WebviewWindowBuilder::position().
    let window = WebviewWindowBuilder::new(&app, &window_label, WebviewUrl::App(url.into()))
        .title("Sticked Note")
        .inner_size(width, height)
        .min_inner_size(320.0, 200.0)
        .max_inner_size(800.0, 600.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build();

    match window {
        Ok(win) => {
            if let Some((x, y)) = saved_position {
                let _ = win.set_position(tauri::Position::Physical(PhysicalPosition::new(
                    x as i32, y as i32,
                )));
            } else {
                let _ = win.center();
            }
            let _ = win.show();
            Ok(true)
        }
        Err(e) => Err(format!("Failed to create sticked window: {}", e)),
    }
}

pub fn create_sticked_window_centered(app: AppHandle, note: StickedNote) -> Result<bool, String> {
    let window_label = format!("sticked-{}", note.id);

    if app.get_webview_window(&window_label).is_some() {
        return Ok(true);
    }

    let (width, height) = note.size.unwrap_or((400.0, 280.0));
    let url = format!("index.html?window=sticked&id={}", note.id);

    let window = WebviewWindowBuilder::new(&app, &window_label, WebviewUrl::App(url.into()))
        .title("Sticked Note")
        .inner_size(width, height)
        .min_inner_size(320.0, 200.0)
        .max_inner_size(800.0, 600.0)
        .center()
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .build();

    if let Err(e) = window {
        return Err(format!("Failed to create sticked window: {}", e));
    }

    Ok(true)
}

#[tauri::command]
pub fn close_sticked_window(app: AppHandle, id: String) -> Result<bool, String> {
    let window_label = format!("sticked-{}", id);

    if let Some(window) = app.get_webview_window(&window_label) {
        let _ = window.close();
    }

    // Clean up viewing note cache to prevent memory leak
    if id.starts_with("view-") {
        let state = app.state::<AppState>();
        let mut viewing_notes = state
            .viewing_notes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        viewing_notes.remove(&id);
    }

    Ok(true)
}

#[tauri::command]
pub async fn pin_capture_note(
    app: AppHandle,
    content: String,
    folder: String,
) -> Result<StickedNote, String> {
    // Read saved viewing position so the pinned note opens where the last
    // sticked/viewing window was, not always centered.
    let saved = settings::load_settings_from_file().ok();
    let saved_pos = saved.as_ref().and_then(|s| s.viewing_window_position);
    let saved_size = saved.as_ref().and_then(|s| s.viewing_window_size);

    let mut note = sticked_notes::create_sticked_note(content, folder, None)?;

    // Use saved viewing position if it's on a connected monitor, otherwise center.
    let use_saved = saved_pos.is_some_and(|(x, y)| {
        let (w, h) = saved_size.unwrap_or((400.0, 280.0));
        is_window_visible_on_any_monitor(&app, x, y, w, h)
    });

    if let (true, Some((x, y))) = (use_saved, saved_pos) {
        note.position = Some((x, y));
        if let Some((w, h)) = saved_size {
            note.size = Some((w, h));
        }
        create_sticked_window(app.clone(), note.clone())?;
    } else {
        create_sticked_window_centered(app.clone(), note.clone())?;
    }

    // Persist the actual window position/size so it restores correctly
    let window_label = format!("sticked-{}", note.id);
    if let Some(win) = app.get_webview_window(&window_label) {
        if let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) {
            let _ = sticked_notes::update_sticked_note(
                note.id.clone(),
                None,
                None,
                Some((pos.x as f64, pos.y as f64)),
                Some((size.width as f64, size.height as f64)),
            );

            // Keep the global viewing geometry in sync.
            let scale = win.scale_factor().unwrap_or(1.0);
            let lw = size.width as f64 / scale;
            let lh = size.height as f64 / scale;
            let _ = settings::save_viewing_window_geometry(lw, lh, pos.x as f64, pos.y as f64);
        }
    }

    if let Some(window) = app.get_webview_window("postit") {
        let _ = window.hide();
    }

    Ok(note)
}

fn viewing_note_id(path: &str) -> String {
    // Preserve path identity without introducing URL or window-label delimiters.
    format!(
        "view-{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(path)
    )
}

#[tauri::command]
pub async fn open_note_for_viewing(
    app: AppHandle,
    content: String,
    folder: String,
    path: String,
) -> Result<bool, String> {
    open_note_window(app, content, folder, path, None, true)
}

fn open_note_window(
    app: AppHandle,
    content: String,
    folder: String,
    path: String,
    origin: Option<(f64, f64)>,
    focus: bool,
) -> Result<bool, String> {
    {
        let state = app.state::<AppState>();
        remember_last_note(&state, &path, &folder);
    }

    let id = viewing_note_id(&path);
    let window_label = format!("sticked-{}", id);

    if app.get_webview_window(&window_label).is_some() {
        reveal_sticker(&app, &window_label, focus);
        return Ok(true);
    }

    let saved_place = note_geometry::parse_note_geometry(&content);
    let display = note_geometry::strip_note_geometry(&content);
    {
        let state = app.state::<AppState>();
        let mut viewing_notes = state
            .viewing_notes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        viewing_notes.insert(
            id.clone(),
            crate::state::ViewingNoteContent {
                id: id.clone(),
                content: display,
                folder,
                path: path.clone(),
            },
        );
    }

    let url = format!("index.html?window=sticked&id={}&viewing=true", id);

    let saved_settings = settings::load_settings_from_file().ok();
    let (width, height) = saved_place
        .as_ref()
        .map(|geometry| note_geometry::clamp_note_size(geometry.width, geometry.height))
        .or_else(|| saved_settings.as_ref().and_then(|s| s.viewing_window_size))
        .unwrap_or((450.0, 320.0));
    let saved_position = saved_settings
        .as_ref()
        .and_then(|s| s.viewing_window_position);

    // Build hidden — we position after creation using PhysicalPosition to avoid
    // the logical/physical mismatch in WebviewWindowBuilder::position().
    let builder = WebviewWindowBuilder::new(&app, &window_label, WebviewUrl::App(url.into()))
        .title("View Note")
        .inner_size(width, height)
        .min_inner_size(320.0, 200.0)
        .max_inner_size(800.0, 600.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false);

    let window = builder.build();

    match window {
        Ok(win) => {
            // A note that remembers its monitor ignores the shared viewing
            // position and a batch cascade. Without that memory, a batch open
            // still spreads notes, and a single open keeps the last position.
            if let Some(geometry) = saved_place {
                let (x, y) = note_geometry::place_on_screen(&geometry, &screen_frames(&win));
                let _ = win.set_position(tauri::Position::Physical(PhysicalPosition::new(
                    x as i32, y as i32,
                )));
            } else if let Some((x, y)) = origin {
                let _ = win.set_position(tauri::Position::Physical(PhysicalPosition::new(
                    x as i32, y as i32,
                )));
            } else {
                let positioned = saved_position.is_some_and(|(x, y)| {
                    is_window_visible_on_any_monitor(&app, x, y, width, height)
                });
                if let (true, Some((x, y))) = (positioned, saved_position) {
                    let _ = win.set_position(tauri::Position::Physical(PhysicalPosition::new(
                        x as i32, y as i32,
                    )));
                } else {
                    let _ = win.center();
                }
            }

            let _ = win.show();
            if focus {
                let _ = win.set_focus();
            }
            reveal_sticker(&app, &window_label, focus);
            Ok(true)
        }
        Err(e) => Err(format!("Failed to create viewing window: {}", e)),
    }
}

fn reveal_sticker(app: &AppHandle, label: &str, focus: bool) {
    let app_for_work = app.clone();
    let label = label.to_string();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = app_for_work.get_webview_window(&label) else {
            return;
        };
        let _ = window.show();
        // A sticker sent behind the desktop stays at the icon level until raised.
        let _ = place_sticker(&window, false);
        if focus {
            let _ = window.set_focus();
        }
    });
}

const NO_NOTE_SAVED: &str = "No note saved yet";

/// Opens the note the user touched most recently. A deleted note is skipped
/// and the previous one opens instead. With no session history, the newest
/// file still in the Stix folder is used.
pub fn open_latest_note(app: AppHandle) -> Result<bool, String> {
    let mut skip = Vec::new();
    loop {
        let notes = live_notes()?;
        let recent = recent_paths(&app);
        let Some(note) = pick_note_path(&recent, &notes, &skip).cloned() else {
            return Err(NO_NOTE_SAVED.to_string());
        };
        match notes::get_note_content_inner(&note.path) {
            Ok(content) => {
                return open_note_window(app, content, note.folder, note.path, None, true);
            }
            Err(error) => {
                eprintln!("Skipped {}: {error}", note.path);
                app.state::<AppState>().forget_note(&note.path);
                skip.push(note.path);
            }
        }
    }
}

/// Plain left click. A saved note opens the latest file. With an empty folder
/// the first click starts one unsaved card, and the next click brings that
/// card forward instead of stacking another.
pub fn open_latest_note_or_capture(
    app: AppHandle,
    anchor: Option<(f64, f64)>,
) -> Result<(), String> {
    match open_latest_note(app.clone()) {
        Ok(_) => Ok(()),
        Err(error) if error == NO_NOTE_SAVED => {
            if reveal_open_capture(&app) {
                Ok(())
            } else {
                open_fresh_capture(&app, anchor)
            }
        }
        Err(error) => Err(error),
    }
}

fn capture_order() -> &'static std::sync::Mutex<Vec<String>> {
    static ORDER: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> = std::sync::OnceLock::new();
    ORDER.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

fn remember_capture(label: &str) {
    let mut order = capture_order()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    order.retain(|existing| existing != label);
    order.push(label.to_string());
}

/// Newest capture label that is still open. Creation order is oldest first.
pub(crate) fn pick_open_capture(
    created_newest_last: &[String],
    mut is_open: impl FnMut(&str) -> bool,
) -> Option<&String> {
    created_newest_last
        .iter()
        .rev()
        .find(|label| is_open(label))
}

fn reveal_open_capture(app: &AppHandle) -> bool {
    let label = {
        let mut order = capture_order()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let label =
            pick_open_capture(&order, |label| app.get_webview_window(label).is_some()).cloned();
        order.retain(|label| app.get_webview_window(label).is_some());
        label
    };
    let Some(label) = label else {
        return false;
    };
    reveal_sticker(app, &label, true);
    true
}

/// Opens every unlocked note stored under the configured Stix folder.
pub fn show_stix_notes(app: AppHandle) -> Result<(), String> {
    let notes = live_notes()?;
    let frame = monitor_frame(&app);
    // Newest is index 0. Create it last so it stays above the others.
    for (index, note) in notes.into_iter().enumerate().rev() {
        let content = match notes::get_note_content_inner(&note.path) {
            Ok(content) => content,
            Err(error) => {
                eprintln!("Skipped {}: {error}", note.path);
                continue;
            }
        };
        let origin = cascade_origin(index, frame.0, frame.1, frame.2, frame.3, frame.4);
        if let Err(error) = open_note_window(
            app.clone(),
            content,
            note.folder,
            note.path.clone(),
            Some(origin),
            index == 0,
        ) {
            eprintln!("Skipped {}: {error}", note.path);
        }
    }
    Ok(())
}

/// Shows every note in the Stix folder, or hides them when they are already up.
pub fn toggle_stix_notes(app: AppHandle) -> Result<(), String> {
    let notes = live_notes()?;
    if should_hide_all(notes.len(), visible_note_windows(&app, &notes)) {
        for note in notes {
            let label = format!("sticked-{}", viewing_note_id(&note.path));
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.hide();
            }
        }
        return Ok(());
    }
    show_stix_notes(app)
}

#[derive(Clone)]
struct LiveNote {
    path: String,
    folder: String,
    modified: std::time::SystemTime,
}

fn live_notes() -> Result<Vec<LiveNote>, String> {
    let root = crate::commands::folders::get_stix_folder()?;
    Ok(list_note_files(&root))
}

fn list_note_files(root: &std::path::Path) -> Vec<LiveNote> {
    let mut notes = Vec::new();
    collect_note_files(root, root, &mut notes);
    notes.sort_by_key(|note| std::cmp::Reverse(note.modified));
    notes
}

fn collect_note_files(root: &std::path::Path, dir: &std::path::Path, into: &mut Vec<LiveNote>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let hidden = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'));
            if !hidden {
                collect_note_files(root, &path, into);
            }
            continue;
        }
        if !crate::commands::path_security::is_visible_note_path(root, &path) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if crate::commands::note_lock::is_locked_content(&content) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        into.push(LiveNote {
            path: path.to_string_lossy().to_string(),
            folder: crate::commands::folders::note_folder(root, &path),
            modified,
        });
    }
}

/// Newest session note that still exists. Missing paths are skipped, then the
/// newest file on disk is used.
fn pick_note_path<'a>(
    recent_oldest_first: &[String],
    files_newest_first: &'a [LiveNote],
    skip: &[String],
) -> Option<&'a LiveNote> {
    let skipped = |path: &str| skip.iter().any(|item| paths_match(item, path));
    for path in recent_oldest_first.iter().rev() {
        if skipped(path) {
            continue;
        }
        if let Some(note) = files_newest_first
            .iter()
            .find(|note| paths_match(&note.path, path))
        {
            return Some(note);
        }
    }
    files_newest_first.iter().find(|note| !skipped(&note.path))
}

fn paths_match(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let Ok(left) = std::fs::canonicalize(left) else {
        return false;
    };
    let Ok(right) = std::fs::canonicalize(right) else {
        return false;
    };
    left == right
}

fn recent_paths(app: &AppHandle) -> Vec<String> {
    app.state::<AppState>()
        .recent_notes
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .map(|note| note.path.clone())
        .collect()
}

fn visible_note_windows(app: &AppHandle, notes: &[LiveNote]) -> usize {
    notes
        .iter()
        .filter(|note| {
            let label = format!("sticked-{}", viewing_note_id(&note.path));
            app.get_webview_window(&label)
                .and_then(|window| window.is_visible().ok())
                .unwrap_or(false)
        })
        .count()
}

fn screen_frame(monitor: &tauri::Monitor) -> ScreenFrame {
    let position = monitor.position();
    let size = monitor.size();
    ScreenFrame {
        x: position.x as f64,
        y: position.y as f64,
        w: size.width as f64,
        h: size.height as f64,
        scale: monitor.scale_factor(),
    }
}

fn screen_frames(window: &WebviewWindow) -> Vec<ScreenFrame> {
    let Ok(monitors) = window.available_monitors() else {
        return Vec::new();
    };
    let primary = window.primary_monitor().ok().flatten();
    let mut frames: Vec<_> = monitors.iter().map(screen_frame).collect();
    if let Some(primary) = primary.as_ref() {
        let preferred = screen_frame(primary);
        frames.sort_by_key(|frame| {
            if note_geometry::same_screen(frame, &preferred) {
                0
            } else {
                1
            }
        });
    }
    frames
}

fn monitor_for_anchor(app: &AppHandle, anchor: Option<(f64, f64)>) -> (f64, f64, f64, f64, f64) {
    let fallback = (0.0, 0.0, 1440.0, 900.0, 1.0);
    let Some(window) = app.webview_windows().into_values().next() else {
        return fallback;
    };
    let frames = screen_frames(&window);
    let chosen = anchor
        .and_then(|(x, y)| {
            frames.iter().find(|frame| {
                x >= frame.x && y >= frame.y && x < frame.x + frame.w && y < frame.y + frame.h
            })
        })
        .or_else(|| frames.first());
    match chosen {
        Some(frame) => (frame.x, frame.y, frame.w, frame.h, frame.scale),
        None => fallback,
    }
}

struct StackSlots {
    counts: Vec<(i64, i64, i64, i64, usize)>,
}

impl StackSlots {
    const fn new() -> Self {
        Self { counts: Vec::new() }
    }

    fn next(&mut self, key: (i64, i64, i64, i64)) -> usize {
        if let Some(entry) = self
            .counts
            .iter_mut()
            .find(|entry| (entry.0, entry.1, entry.2, entry.3) == key)
        {
            let slot = entry.4;
            entry.4 = slot.saturating_add(1);
            return slot;
        }
        self.counts.push((key.0, key.1, key.2, key.3, 1));
        0
    }
}

fn next_stack_slot(x: f64, y: f64, w: f64, h: f64) -> usize {
    use std::sync::Mutex;
    static SLOTS: Mutex<StackSlots> = Mutex::new(StackSlots::new());
    let key = (
        x.round() as i64,
        y.round() as i64,
        w.round() as i64,
        h.round() as i64,
    );
    SLOTS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .next(key)
}

fn monitor_frame(app: &AppHandle) -> (f64, f64, f64, f64, f64) {
    let fallback = (0.0, 0.0, 1440.0, 900.0, 1.0);
    let Some(window) = app.get_webview_window("postit") else {
        return fallback;
    };
    let Ok(monitors) = window.available_monitors() else {
        return fallback;
    };
    let Some(monitor) = monitors.first() else {
        return fallback;
    };
    let position = monitor.position();
    let size = monitor.size();
    (
        position.x as f64,
        position.y as f64,
        size.width as f64,
        size.height as f64,
        monitor.scale_factor(),
    )
}

/// Hide is only right when every note already has its window on screen.
pub(crate) fn should_hide_all(total: usize, visible: usize) -> bool {
    total > 0 && visible == total
}

/// Physical origin for a new note, stacked from the monitor's top-right corner.
/// Each next note sits lower and slightly further left. A stack that reaches
/// the bottom starts again at the top, in the next column to the left.
pub(crate) fn stack_origin(
    slot: usize,
    frame: (f64, f64, f64, f64, f64),
    window: (f64, f64),
) -> (f64, f64) {
    let (frame_x, frame_y, frame_w, frame_h, scale) = frame;
    let (window_w, window_h) = window;
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let margin = 24.0 * scale;
    let top_margin = 64.0 * scale;
    let step_x = 16.0 * scale;
    let step_y = 32.0 * scale;
    let column_gap = 48.0 * scale;
    let win_w = window_w.max(1.0) * scale;
    let win_h = window_h.max(1.0) * scale;
    let top = frame_y + top_margin;
    let right = frame_x + frame_w - win_w - margin;
    let max_y = frame_y + frame_h - win_h - margin;
    let rows = ((max_y - top).max(0.0) / step_y).floor() as usize + 1;
    let column_stride = rows as f64 * step_x + column_gap;
    let min_x = frame_x + margin;
    let cols = ((right - min_x).max(0.0) / column_stride).floor() as usize + 1;
    let slots = rows.saturating_mul(cols).max(1);
    let slot = slot % slots;
    let col = slot / rows;
    let row = slot % rows;
    (
        right - row as f64 * step_x - col as f64 * column_stride,
        top + row as f64 * step_y,
    )
}

/// Physical origin for the `index`th note, stepped across the monitor.
pub(crate) fn cascade_origin(
    index: usize,
    frame_x: f64,
    frame_y: f64,
    frame_w: f64,
    frame_h: f64,
    scale: f64,
) -> (f64, f64) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let step_x = 64.0 * scale;
    let step_y = 52.0 * scale;
    let margin = 24.0 * scale;
    let window_w = 450.0 * scale;
    let window_h = 320.0 * scale;
    let cols = ((frame_w - window_w - margin) / step_x).floor().max(1.0) as usize;
    let rows = ((frame_h - window_h - margin) / step_y).floor().max(1.0) as usize;
    let slots = cols.saturating_mul(rows).max(1);
    let slot = index % slots;
    let col = slot % cols;
    let row = slot / cols;
    (
        frame_x + margin + col as f64 * step_x,
        frame_y + margin + row as f64 * step_y,
    )
}

#[tauri::command]
pub fn get_viewing_note_content(app: AppHandle, id: String) -> Result<serde_json::Value, String> {
    let state = app.state::<AppState>();
    let viewing_notes = state
        .viewing_notes
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    if let Some(note) = viewing_notes.get(&id) {
        Ok(serde_json::json!({
            "id": note.id,
            "content": note.content,
            "folder": note.folder,
            "path": note.path
        }))
    } else {
        Err("Viewing note content not found".to_string())
    }
}

#[tauri::command]
pub async fn transfer_to_capture(
    app: AppHandle,
    content: String,
    folder: String,
) -> Result<bool, String> {
    let window = app
        .get_webview_window("postit")
        .ok_or("Postit window not found")?;
    let id = uuid::Uuid::new_v4().to_string();
    let (sender, receiver) = std::sync::mpsc::channel();
    let listener = app.once(format!("capture-transfer-result-{id}"), move |event| {
        let result = serde_json::from_str::<CaptureTransferResult>(event.payload())
            .map_err(|error| error.to_string())
            .and_then(|result| result.error.map_or(Ok(true), Err));
        let _ = sender.send(result);
    });
    let result = async {
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        window
            .emit(
                "transfer-content",
                serde_json::json!({
                    "id": id,
                    "content": content,
                    "folder": folder
                }),
            )
            .map_err(|error| error.to_string())?;
        // Do not block the event loop that must deliver the receiver's reply.
        // A missing listener or failed save leaves the source pin untouched.
        tauri::async_runtime::spawn_blocking(move || {
            receiver
                .recv_timeout(std::time::Duration::from_secs(20))
                .map_err(|_| {
                    "Capture did not accept the note; the source is still pinned".to_string()
                })?
        })
        .await
        .map_err(|error| error.to_string())?
    }
    .await;
    app.unlisten(listener);
    result
}

#[derive(serde::Deserialize)]
struct CaptureTransferResult {
    error: Option<String>,
}

#[tauri::command]
pub fn open_command_palette(app: AppHandle) -> Result<bool, String> {
    show_command_palette(&app);
    Ok(true)
}

#[tauri::command]
pub fn open_search(app: AppHandle) -> Result<bool, String> {
    show_command_palette(&app);
    Ok(true)
}

#[tauri::command]
pub fn open_manager(app: AppHandle) -> Result<bool, String> {
    show_command_palette(&app);
    Ok(true)
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<bool, String> {
    show_settings(&app);
    Ok(true)
}

#[tauri::command]
pub async fn reopen_last_note(app: AppHandle) -> Result<bool, String> {
    open_latest_note(app)
}

pub fn show_apple_notes_picker(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("apple-notes-picker") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let window = WebviewWindowBuilder::new(
        app,
        "apple-notes-picker",
        WebviewUrl::App("index.html?window=apple-notes-picker".into()),
    )
    .title("Import from Apple Notes")
    .inner_size(550.0, 500.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .center()
    .build();

    if let Ok(win) = window {
        let app_handle = app.clone();
        win.on_window_event(move |event| {
            if let tauri::WindowEvent::Focused(focused) = event {
                if !focused {
                    if let Some(w) = app_handle.get_webview_window("apple-notes-picker") {
                        let _ = w.close();
                    }
                }
            }
        });
    }
}

#[tauri::command]
pub fn show_apple_notes_picker_cmd(app: AppHandle) -> Result<bool, String> {
    show_apple_notes_picker(&app);
    Ok(true)
}

pub fn restore_sticked_notes(app: &AppHandle) {
    if let Ok(notes) = sticked_notes::list_sticked_notes() {
        for note in notes {
            let _ = create_sticked_window(app.clone(), note);
        }
    }
}

fn is_sticker_label(label: &str) -> bool {
    label == "postit" || label.starts_with("sticked-") || label.starts_with("capture-")
}

fn sunk_stickers() -> &'static std::sync::Mutex<Vec<String>> {
    static SUNK: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> = std::sync::OnceLock::new();
    SUNK.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

fn remember_sunk(label: &str) {
    let mut sunk = sunk_stickers()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    sunk.retain(|existing| existing != label);
    sunk.push(label.to_string());
}

#[cfg(target_os = "macos")]
fn window_level(key: i32) -> isize {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowLevelForKey(key: i32) -> i32;
    }
    // CGWindowLevelKey: desktop icons = 18, floating = 5.
    // Key 2 is the wallpaper level, so a sticker placed there disappears.
    unsafe { CGWindowLevelForKey(key) as isize }
}

#[cfg(target_os = "macos")]
fn ns_window(window: &tauri::WebviewWindow) -> Result<&objc2_app_kit::NSWindow, String> {
    let pointer = window.ns_window().map_err(|error| error.to_string())?;
    // SAFETY: Tauri's ns_window() is the live NSWindow for this webview.
    unsafe { pointer.cast::<objc2_app_kit::NSWindow>().as_ref() }
        .ok_or_else(|| "Sticker window handle is null".to_string())
}

#[cfg(target_os = "macos")]
fn place_sticker(window: &tauri::WebviewWindow, behind: bool) -> Result<(), String> {
    let _ = window.set_always_on_top(!behind);
    let level = window_level(if behind { 18 } else { 5 });
    let ns_window = ns_window(window)?;
    // Panels hide themselves when the app deactivates. A sunk sticker must stay
    // on the desktop, under ordinary windows.
    ns_window.setHidesOnDeactivate(false);
    ns_window.setLevel(level);
    if behind {
        ns_window.orderBack(None);
        if let Some(marker) = objc2::MainThreadMarker::new() {
            objc2_app_kit::NSApplication::sharedApplication(marker).deactivate();
        }
    } else {
        ns_window.orderFrontRegardless();
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn place_sticker(window: &tauri::WebviewWindow, behind: bool) -> Result<(), String> {
    window
        .set_always_on_top(!behind)
        .map_err(|error| error.to_string())
}

fn sink_sticker(window: &tauri::WebviewWindow) -> Result<(), String> {
    let label = window.label().to_string();
    if !is_sticker_label(&label) {
        return Ok(());
    }
    place_sticker(window, true)?;
    remember_sunk(&label);
    Ok(())
}

fn raise_last_sunk(app: &AppHandle) -> Result<(), String> {
    let label = {
        let mut sunk = sunk_stickers()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        loop {
            let Some(label) = sunk.pop() else {
                return Ok(());
            };
            if app.get_webview_window(&label).is_some() {
                break label;
            }
        }
    };
    let Some(window) = app.get_webview_window(&label) else {
        return Ok(());
    };
    let _ = window.show();
    let _ = window.unminimize();
    place_sticker(&window, false)?;
    let _ = window.set_focus();
    Ok(())
}

fn on_main_thread<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = sender.send(work());
    })
    .map_err(|error| error.to_string())?;
    receiver.recv().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn sink_focused_sticker(window: tauri::WebviewWindow) -> Result<(), String> {
    let app = window.app_handle().clone();
    on_main_thread(&app, move || sink_sticker(&window))?
}

#[tauri::command]
pub fn raise_last_sticker(app: AppHandle) -> Result<(), String> {
    let app_for_work = app.clone();
    on_main_thread(&app, move || raise_last_sunk(&app_for_work))?
}

/// Grave+Escape is not a modifier chord, so the global-shortcut plugin cannot
/// register it. A session event tap sees that physical key (US ` and Russian
/// ё) even when the sticker is buried under other apps. Only that chord is
/// swallowed; a plain Escape is returned to the frontmost app.
pub fn install_sticker_order_monitor(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
        use std::sync::OnceLock;

        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGEventTapCreate(
                tap: u32,
                place: u32,
                options: u32,
                events_of_interest: u64,
                callback: unsafe extern "C" fn(
                    *mut std::ffi::c_void,
                    u32,
                    *mut std::ffi::c_void,
                    *mut std::ffi::c_void,
                ) -> *mut std::ffi::c_void,
                user_info: *mut std::ffi::c_void,
            ) -> *mut std::ffi::c_void;
            fn CGEventTapEnable(tap: *mut std::ffi::c_void, enable: bool);
            fn CGEventGetIntegerValueField(event: *mut std::ffi::c_void, field: u32) -> i64;
        }
        #[link(name = "CoreFoundation", kind = "framework")]
        extern "C" {
            static kCFRunLoopCommonModes: *const std::ffi::c_void;
            fn CFMachPortCreateRunLoopSource(
                allocator: *const std::ffi::c_void,
                port: *mut std::ffi::c_void,
                order: isize,
            ) -> *mut std::ffi::c_void;
            fn CFRunLoopGetCurrent() -> *mut std::ffi::c_void;
            fn CFRunLoopAddSource(
                run_loop: *mut std::ffi::c_void,
                source: *mut std::ffi::c_void,
                mode: *const std::ffi::c_void,
            );
        }

        const KEY_DOWN: u32 = 10;
        const KEY_UP: u32 = 11;
        const TAP_DISABLED: u32 = 0xFFFF_FFFE;
        const KEYBOARD_KEYCODE: u32 = 9;
        const GRAVE: i64 = 50;
        const ESCAPE: i64 = 53;

        static APP: OnceLock<AppHandle> = OnceLock::new();
        static GRAVE_DOWN: AtomicBool = AtomicBool::new(false);
        static TAP: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

        unsafe extern "C" fn on_key(
            _proxy: *mut std::ffi::c_void,
            kind: u32,
            event: *mut std::ffi::c_void,
            _user: *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void {
            if kind == TAP_DISABLED {
                let tap = TAP.load(Ordering::Relaxed);
                if !tap.is_null() {
                    CGEventTapEnable(tap, true);
                }
                return event;
            }
            let code = CGEventGetIntegerValueField(event, KEYBOARD_KEYCODE);
            if code == GRAVE {
                GRAVE_DOWN.store(kind == KEY_DOWN, Ordering::Relaxed);
                return event;
            }
            if kind == KEY_DOWN && code == ESCAPE && GRAVE_DOWN.load(Ordering::Relaxed) {
                if let Some(app) = APP.get().cloned() {
                    let app_for_work = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        if let Err(error) = raise_last_sunk(&app_for_work) {
                            eprintln!("Failed to raise sticker: {error}");
                        }
                    });
                }
                return std::ptr::null_mut();
            }
            event
        }

        if APP.set(app.clone()).is_err() {
            return;
        }
        let mask = (1_u64 << KEY_DOWN) | (1_u64 << KEY_UP);
        let tap = unsafe {
            // Session tap, default options so only the grave+Escape chord is
            // swallowed. A plain Escape is returned unchanged.
            CGEventTapCreate(1, 0, 0, mask, on_key, std::ptr::null_mut())
        };
        if tap.is_null() {
            eprintln!("Sticker raise shortcut needs Accessibility permission");
            return;
        }
        TAP.store(tap, Ordering::Relaxed);
        unsafe {
            let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            if !source.is_null() {
                CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
            }
            CGEventTapEnable(tap, true);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cascade_origin, list_note_files, pick_note_path, pick_open_capture, remember_last_note,
        should_hide_all, stack_origin, viewing_note_id, LiveNote, SETTINGS_WINDOW_MIN_WIDTH,
        SETTINGS_WINDOW_WIDTH,
    };
    use crate::state::AppState;

    #[test]
    fn viewing_ids_keep_similarly_named_files_in_separate_windows() {
        let paths = [
            "/notes/a.b.md",
            "/notes/a b.md",
            "/notes/a-b.md",
            "/notes/a/b.md",
            "/notes/a\\b.md",
        ];
        let ids: std::collections::HashSet<_> = paths.into_iter().map(viewing_note_id).collect();
        assert_eq!(ids.len(), paths.len());
    }

    #[test]
    fn viewing_ids_are_safe_in_window_labels_and_url_parameters() {
        let id = viewing_note_id("/notes/你好 & question?#.md");
        assert!(id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    }

    #[test]
    fn remember_last_note_updates_state_for_shortcuts() {
        let state = AppState::new();
        remember_last_note(&state, "/tmp/stix/foo.md", "Inbox");

        let last = state
            .last_saved_note
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let note = last.as_ref().expect("last note should be set");
        assert_eq!(note.path, "/tmp/stix/foo.md");
        assert_eq!(note.folder, "Inbox");
        drop(last);
        let recent = state
            .recent_notes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert_eq!(recent.len(), 1);
        drop(recent);
        state.forget_note("/tmp/stix/foo.md");
        assert!(state
            .recent_notes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_empty());
    }

    fn sample(path: &str) -> LiveNote {
        LiveNote {
            path: path.to_string(),
            folder: "Inbox".to_string(),
            modified: std::time::SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn deleted_latest_note_falls_back_to_the_previous_one() {
        let files = vec![sample("/notes/older.md"), sample("/notes/latest.md")];
        let recent = vec![
            "/notes/older.md".to_string(),
            "/notes/latest.md".to_string(),
        ];
        let picked = pick_note_path(&recent, &files, &[]).unwrap();
        assert_eq!(picked.path, "/notes/latest.md");

        let remaining = vec![sample("/notes/older.md")];
        let picked = pick_note_path(&recent, &remaining, &[]).unwrap();
        assert_eq!(picked.path, "/notes/older.md");
    }

    #[test]
    fn without_history_the_newest_file_opens() {
        let mut older = sample("/notes/older.md");
        older.modified += std::time::Duration::from_secs(1);
        let mut newer = sample("/notes/newer.md");
        newer.modified += std::time::Duration::from_secs(5);
        // Caller passes files newest first.
        let files = vec![newer, older];
        let picked = pick_note_path(&[], &files, &[]).unwrap();
        assert_eq!(picked.path, "/notes/newer.md");
    }

    #[test]
    fn live_note_scan_skips_trash_and_orders_by_mtime() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("stix-notes-{nonce}"));
        let inbox = root.join("Inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::create_dir_all(root.join(".trash")).unwrap();
        let older = inbox.join("older.md");
        let newer = inbox.join("newer.md");
        std::fs::write(&older, "older").unwrap();
        std::fs::write(&newer, "newer").unwrap();
        std::fs::write(root.join(".trash/gone.md"), "gone").unwrap();
        let earlier = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10);
        let later = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(20);
        std::fs::File::open(&older)
            .unwrap()
            .set_modified(earlier)
            .unwrap();
        std::fs::File::open(&newer)
            .unwrap()
            .set_modified(later)
            .unwrap();

        let notes = list_note_files(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(notes.len(), 2);
        assert!(notes[0].path.ends_with("newer.md"));
        assert!(notes[1].path.ends_with("older.md"));
        assert_eq!(notes[0].folder, "Inbox");
    }

    #[test]
    fn settings_window_min_width_is_large_enough_for_full_menu_bar() {
        const {
            assert!(SETTINGS_WINDOW_MIN_WIDTH >= 760.0);
            assert!(SETTINGS_WINDOW_WIDTH > SETTINGS_WINDOW_MIN_WIDTH);
        }
    }

    #[test]
    fn hide_all_only_when_every_note_is_already_visible() {
        assert!(!should_hide_all(0, 0));
        assert!(!should_hide_all(3, 2));
        assert!(should_hide_all(3, 3));
    }

    #[test]
    fn cascade_spreads_notes_across_the_monitor() {
        let first = cascade_origin(0, 0.0, 40.0, 2000.0, 1200.0, 2.0);
        let second = cascade_origin(1, 0.0, 40.0, 2000.0, 1200.0, 2.0);
        assert_ne!(first, second);
        assert!(first.0 >= 0.0 && first.1 >= 40.0);
        assert!(second.0 < 2000.0 && second.1 < 1240.0);
        assert!(second.0 > first.0);
    }

    #[test]
    fn fresh_notes_stack_from_the_top_right() {
        let frame = (0.0, 0.0, 2560.0, 1440.0, 2.0);
        let window = (400.0, 280.0);
        let first = stack_origin(0, frame, window);
        let second = stack_origin(1, frame, window);
        assert!((first.0 - (2560.0 - 800.0 - 48.0)).abs() < 1.0);
        assert!((first.1 - 128.0).abs() < 1.0);
        assert!(second.1 > first.1);
        assert!(second.0 < first.0);

        let mut wrapped = None;
        for slot in 1..40 {
            let origin = stack_origin(slot, frame, window);
            if (origin.1 - first.1).abs() < 1.0 && origin.0 < second.0 {
                wrapped = Some(origin);
                break;
            }
        }
        let wrapped = wrapped.expect("the stack should start a new column at the top");
        assert!(wrapped.0 < first.0);
    }

    #[test]
    fn plain_click_reuses_the_newest_unsaved_note() {
        let created = vec![
            "capture-old".to_string(),
            "capture-new".to_string(),
            "capture-closed".to_string(),
        ];
        let open = ["capture-old", "capture-new"];
        let picked = pick_open_capture(&created, |label| open.contains(&label));
        assert_eq!(picked.map(String::as_str), Some("capture-new"));

        let only_old = vec!["capture-old".to_string(), "capture-closed".to_string()];
        let picked = pick_open_capture(&only_old, |label| label == "capture-old");
        assert_eq!(picked.map(String::as_str), Some("capture-old"));

        assert!(pick_open_capture(&["capture-closed".to_string()], |_| false).is_none());
    }
}
