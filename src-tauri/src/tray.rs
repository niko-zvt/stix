use crate::commands::{settings, stats};
use crate::windows::{self, show_postit_with_folder};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle};

/// What a left click on the status item should do. A right click is absent
/// here because AppKit opens the menu for that button on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayPointerAction {
    LastNote,
    NewNote,
    ShowAll,
}

fn pointer_action(
    button: MouseButton,
    state: MouseButtonState,
    command: bool,
    shift: bool,
) -> Option<TrayPointerAction> {
    if state != MouseButtonState::Down || button != MouseButton::Left {
        return None;
    }
    if command && shift {
        Some(TrayPointerAction::ShowAll)
    } else if command {
        Some(TrayPointerAction::NewNote)
    } else {
        Some(TrayPointerAction::LastNote)
    }
}

pub fn setup_tray(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let streak_days = stats::calculate_and_persist_capture_streak().unwrap_or_else(|e| {
        eprintln!("Failed to compute capture streak: {}", e);
        0
    });
    let streak_label = stats::format_capture_streak_label(streak_days);

    let quit = MenuItem::with_id(app, "quit", "Quit Stix", true, None::<&str>)?;
    let new_note = MenuItem::with_id(app, "new_note", "New Note", true, None::<&str>)?;
    let show_hide_all =
        MenuItem::with_id(app, "show_hide_all", "Show/Hide All", true, None::<&str>)?;
    let open_editor = MenuItem::with_id(app, "open_editor", "Open Editor", true, None::<&str>)?;
    let capture_streak =
        MenuItem::with_id(app, "capture_streak", &streak_label, false, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &new_note,
            &show_hide_all,
            &open_editor,
            &capture_streak,
            &quit,
        ],
    )?;

    let tray_icon = Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;

    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(tray_icon)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => {
                app.exit(0);
            }
            "new_note" => open_capture(app),
            "show_hide_all" => {
                let app = app.clone();
                spawn_tray(move || {
                    if let Err(error) = windows::toggle_stix_notes(app) {
                        eprintln!("Failed to show or hide notes: {error}");
                    }
                });
            }
            "open_editor" => windows::show_editor(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let TrayIconEvent::Click {
                button,
                button_state,
                position,
                ..
            } = event
            else {
                return;
            };
            let (command, shift) = pointer_modifiers();
            if button_state == MouseButtonState::Down {
                let _ = tray.with_inner_tray_icon(|icon| {
                    icon.set_show_menu_on_left_click(false);
                    icon.set_show_menu_on_right_click(true);
                });
            }
            let Some(action) = pointer_action(button, button_state, command, shift) else {
                return;
            };
            match action {
                TrayPointerAction::NewNote => {
                    if let Err(error) = windows::open_fresh_capture(
                        tray.app_handle(),
                        Some((position.x, position.y)),
                    ) {
                        eprintln!("Failed to open a new note: {error}");
                    }
                }
                TrayPointerAction::LastNote => {
                    let app = tray.app_handle().clone();
                    let anchor = (position.x, position.y);
                    spawn_tray(move || {
                        if let Err(error) = windows::open_latest_note_or_capture(app, Some(anchor))
                        {
                            eprintln!("Failed to open the latest note: {error}");
                        }
                    });
                }
                TrayPointerAction::ShowAll => {
                    let app = tray.app_handle().clone();
                    spawn_tray(move || {
                        if let Err(error) = windows::show_stix_notes(app) {
                            eprintln!("Failed to open notes: {error}");
                        }
                    });
                }
            }
        })
        .build(app)?;

    Ok(())
}

fn open_capture(app: &AppHandle) {
    let settings = settings::get_settings().unwrap_or_default();
    show_postit_with_folder(app, &settings.default_folder);
}

fn spawn_tray(work: impl FnOnce() + Send + 'static) {
    if let Err(error) = std::thread::Builder::new()
        .name("stix-tray".to_string())
        .spawn(work)
    {
        eprintln!("Failed to start tray action: {error}");
    }
}

/// Command and Shift as they are held during the status-item click.
#[cfg(target_os = "macos")]
fn pointer_modifiers() -> (bool, bool) {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceFlagsState(state_id: i32) -> u64;
    }
    // kCGEventSourceStateHIDSystemState. Shift is bit 17, Command is bit 20.
    const HID_SYSTEM_STATE: i32 = 1;
    const SHIFT: u64 = 1 << 17;
    const COMMAND: u64 = 1 << 20;
    // SAFETY: reads the current keyboard flags. It does not take a pointer.
    let flags = unsafe { CGEventSourceFlagsState(HID_SYSTEM_STATE) };
    (flags & COMMAND != 0, flags & SHIFT != 0)
}

#[cfg(not(target_os = "macos"))]
fn pointer_modifiers() -> (bool, bool) {
    (false, false)
}

#[cfg(test)]
mod tests {
    use super::{pointer_action, TrayPointerAction};
    use tauri::tray::{MouseButton, MouseButtonState};

    #[test]
    fn left_click_opens_the_latest_note() {
        assert_eq!(
            pointer_action(MouseButton::Left, MouseButtonState::Down, false, false),
            Some(TrayPointerAction::LastNote)
        );
    }

    #[test]
    fn command_click_opens_a_new_note() {
        assert_eq!(
            pointer_action(MouseButton::Left, MouseButtonState::Down, true, false),
            Some(TrayPointerAction::NewNote)
        );
    }

    #[test]
    fn command_shift_click_opens_every_note() {
        assert_eq!(
            pointer_action(MouseButton::Left, MouseButtonState::Down, true, true),
            Some(TrayPointerAction::ShowAll)
        );
    }

    #[test]
    fn right_click_and_mouse_up_leave_the_menu_alone() {
        assert_eq!(
            pointer_action(MouseButton::Right, MouseButtonState::Down, false, false),
            None
        );
        assert_eq!(
            pointer_action(MouseButton::Right, MouseButtonState::Down, true, true),
            None
        );
        assert_eq!(
            pointer_action(MouseButton::Left, MouseButtonState::Up, true, true),
            None
        );
    }
}
