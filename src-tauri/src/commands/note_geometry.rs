//! Window size and placement stored at the top of a note.
//!
//! The editor, search index, and filename never see these lines. They are
//! written back around the user's text when the note is saved or moved.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteGeometry {
    pub width: f64,
    pub height: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub screen_x: f64,
    pub screen_y: f64,
    pub screen_w: f64,
    pub screen_h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenFrame {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub scale: f64,
}

const SIZE_MARK: &str = "<!-- stix:size ";
const PLACE_MARK: &str = "<!-- stix:place ";

pub fn strip_note_geometry(content: &str) -> String {
    let mut rest = content;
    while let Some((line, after)) = split_first_line(rest) {
        if !is_geometry_line(line) {
            break;
        }
        rest = after;
    }
    rest.to_string()
}

pub fn parse_note_geometry(content: &str) -> Option<NoteGeometry> {
    let mut size = None;
    let mut place = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() && size.is_none() && place.is_none() {
            continue;
        }
        if let Some(parsed) = parse_size_line(trimmed) {
            size = Some(parsed);
            continue;
        }
        if let Some(parsed) = parse_place_line(trimmed) {
            place = Some(parsed);
            continue;
        }
        break;
    }
    let (width, height) = size?;
    let (offset_x, offset_y, screen_x, screen_y, screen_w, screen_h) = place?;
    Some(NoteGeometry {
        width,
        height,
        offset_x,
        offset_y,
        screen_x,
        screen_y,
        screen_w,
        screen_h,
    })
}

pub fn embed_note_geometry(body: &str, geometry: &NoteGeometry) -> String {
    let body = strip_note_geometry(body);
    format!(
        "<!-- stix:size {}x{} -->\n<!-- stix:place {},{} screen {},{} {}x{} -->\n{body}",
        fmt_num(geometry.width),
        fmt_num(geometry.height),
        fmt_num(geometry.offset_x),
        fmt_num(geometry.offset_y),
        fmt_num(geometry.screen_x),
        fmt_num(geometry.screen_y),
        fmt_num(geometry.screen_w),
        fmt_num(geometry.screen_h),
    )
}

/// Keeps a restored note inside the limits the sticker window already enforces.
pub fn clamp_note_size(width: f64, height: f64) -> (f64, f64) {
    (width.clamp(320.0, 800.0), height.clamp(200.0, 600.0))
}

pub fn same_screen(left: &ScreenFrame, right: &ScreenFrame) -> bool {
    near(left.x, right.x) && near(left.y, right.y) && near(left.w, right.w) && near(left.h, right.h)
}

/// Offset on the monitor whose frame matches the file.
/// A missing monitor keeps that offset from the first available screen and
/// pulls the window back inside it.
pub fn place_on_screen(geometry: &NoteGeometry, screens: &[ScreenFrame]) -> (f64, f64) {
    let saved = ScreenFrame {
        x: geometry.screen_x,
        y: geometry.screen_y,
        w: geometry.screen_w,
        h: geometry.screen_h,
        scale: 1.0,
    };
    if let Some(screen) = screens.iter().find(|screen| same_screen(screen, &saved)) {
        return (screen.x + geometry.offset_x, screen.y + geometry.offset_y);
    }
    let Some(screen) = screens.first() else {
        return (
            geometry.screen_x + geometry.offset_x,
            geometry.screen_y + geometry.offset_y,
        );
    };
    let scale = if screen.scale.is_finite() && screen.scale > 0.0 {
        screen.scale
    } else {
        1.0
    };
    let width = geometry.width.max(1.0) * scale;
    let height = geometry.height.max(1.0) * scale;
    let mut x = screen.x + geometry.offset_x;
    let mut y = screen.y + geometry.offset_y;
    let max_x = screen.x + screen.w - width;
    let max_y = screen.y + screen.h - height;
    if x > max_x {
        x = max_x;
    }
    if y > max_y {
        y = max_y;
    }
    if x < screen.x {
        x = screen.x;
    }
    if y < screen.y {
        y = screen.y;
    }
    (x, y)
}

fn is_geometry_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with(SIZE_MARK) || trimmed.starts_with(PLACE_MARK)
}

fn split_first_line(content: &str) -> Option<(&str, &str)> {
    if content.is_empty() {
        return None;
    }
    match content.find('\n') {
        Some(index) => {
            let line = content[..index]
                .strip_suffix('\r')
                .unwrap_or(&content[..index]);
            Some((line, &content[index + 1..]))
        }
        None => Some((content.strip_suffix('\r').unwrap_or(content), "")),
    }
}

fn parse_size_line(line: &str) -> Option<(f64, f64)> {
    let rest = line.strip_prefix(SIZE_MARK)?.strip_suffix("-->")?.trim();
    let (width, height) = rest.split_once('x')?;
    let width = finite(width.trim())?;
    let height = finite(height.trim())?;
    (width > 0.0 && height > 0.0).then_some((width, height))
}

fn parse_place_line(line: &str) -> Option<(f64, f64, f64, f64, f64, f64)> {
    let rest = line.strip_prefix(PLACE_MARK)?.strip_suffix("-->")?.trim();
    let (offset, screen) = rest.split_once(" screen ")?;
    let (origin, size) = screen.trim().split_once(' ')?;
    let (offset_x, offset_y) = split_comma(offset)?;
    let (screen_x, screen_y) = split_comma(origin)?;
    let (screen_w, screen_h) = split_x(size)?;
    (screen_w > 0.0 && screen_h > 0.0)
        .then_some((offset_x, offset_y, screen_x, screen_y, screen_w, screen_h))
}

fn split_comma(value: &str) -> Option<(f64, f64)> {
    let (left, right) = value.trim().split_once(',')?;
    Some((finite(left.trim())?, finite(right.trim())?))
}

fn split_x(value: &str) -> Option<(f64, f64)> {
    let (left, right) = value.trim().split_once('x')?;
    Some((finite(left.trim())?, finite(right.trim())?))
}

fn finite(value: &str) -> Option<f64> {
    let parsed: f64 = value.parse().ok()?;
    parsed.is_finite().then_some(parsed)
}

fn fmt_num(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let rounded = value.round();
    if (value - rounded).abs() < 0.05 {
        format!("{}", rounded as i64)
    } else {
        format!("{value:.1}")
    }
}

fn near(left: f64, right: f64) -> bool {
    (left - right).abs() < 2.0
}

#[cfg(test)]
mod tests {
    use super::{
        embed_note_geometry, parse_note_geometry, place_on_screen, strip_note_geometry,
        NoteGeometry, ScreenFrame,
    };

    fn sample() -> NoteGeometry {
        NoteGeometry {
            width: 400.0,
            height: 280.0,
            offset_x: 48.0,
            offset_y: 24.0,
            screen_x: 0.0,
            screen_y: 0.0,
            screen_w: 2560.0,
            screen_h: 1440.0,
        }
    }

    #[test]
    fn geometry_round_trips_and_leaves_the_note_text() {
        let stored = embed_note_geometry("Hello\n", &sample());
        assert!(stored.starts_with(
            "<!-- stix:size 400x280 -->\n<!-- stix:place 48,24 screen 0,0 2560x1440 -->\n"
        ));
        assert_eq!(strip_note_geometry(&stored), "Hello\n");
        assert_eq!(parse_note_geometry(&stored), Some(sample()));
    }

    #[test]
    fn a_header_only_file_has_no_visible_text() {
        let stored = embed_note_geometry("", &sample());
        assert_eq!(strip_note_geometry(&stored), "");
    }

    #[test]
    fn matching_monitor_keeps_the_saved_offset() {
        let screens = [ScreenFrame {
            x: 0.0,
            y: 0.0,
            w: 2560.0,
            h: 1440.0,
            scale: 2.0,
        }];
        assert_eq!(place_on_screen(&sample(), &screens), (48.0, 24.0));
    }

    #[test]
    fn missing_monitor_uses_the_same_offset_on_the_available_screen() {
        let mut geometry = sample();
        geometry.screen_x = 3000.0;
        geometry.offset_x = 100.0;
        geometry.offset_y = 80.0;
        let screens = [ScreenFrame {
            x: 0.0,
            y: 0.0,
            w: 1280.0,
            h: 800.0,
            scale: 1.0,
        }];
        assert_eq!(place_on_screen(&geometry, &screens), (100.0, 80.0));

        geometry.offset_x = 2000.0;
        geometry.offset_y = 900.0;
        assert_eq!(place_on_screen(&geometry, &screens), (880.0, 520.0));
    }
}
