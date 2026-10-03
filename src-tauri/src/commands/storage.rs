/// Storage abstraction — local filesystem notes, either the default Documents
/// folder or a custom directory chosen in settings.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::settings;

// ── Storage Mode ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StorageMode {
    Local,
    Custom(String),
}

/// Determine the active storage mode from settings.
/// A custom absolute notes directory wins; otherwise notes live in Documents.
pub fn current_mode() -> StorageMode {
    match settings::load_settings_from_file() {
        Ok(s) if !s.notes_directory.is_empty() => {
            let p = PathBuf::from(&s.notes_directory);
            if p.is_absolute() {
                StorageMode::Custom(s.notes_directory)
            } else {
                StorageMode::Local
            }
        }
        _ => StorageMode::Local,
    }
}

/// Resolve the configured root without creating it. Diagnostics use this to
/// report a missing or moved vault instead of silently creating a new one.
pub fn configured_stix_root() -> Result<PathBuf, String> {
    if let Some(root) = super::paths::dev_root()? {
        return Ok(root.join("notes"));
    }
    let preferred = match current_mode() {
        StorageMode::Custom(dir) => {
            let use_as_root = settings::load_settings_from_file()
                .map(|s| s.use_directory_as_root)
                .unwrap_or(false);
            let directory = PathBuf::from(&dir);
            if use_as_root {
                if directory.file_name().and_then(|name| name.to_str()) == Some("Stix") {
                    directory
                        .parent()
                        .map(|parent| parent.join("Stix"))
                        .unwrap_or(directory)
                } else {
                    directory
                }
            } else {
                directory.join("Stix")
            }
        }
        StorageMode::Local => {
            let docs = dirs::document_dir().ok_or("Could not find Documents directory")?;
            docs.join("Stix")
        }
    };
    let config = super::paths::config_dir().ok();
    super::paths::adopt_legacy_notes_root(&preferred, config.as_deref())
}

/// Get the root Stix directory for the current storage mode, creating it for
/// normal application use when necessary.
pub fn stix_root() -> Result<PathBuf, String> {
    let path = configured_stix_root()?;
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

// ── Atomic Writes ─────────────────────────────────────────────────
//
// A truncating `fs::write` leaves a window where a reader sees a half-written
// note. Finder, Obsidian, the file watcher and any future sync agent all read
// this tree, so notes go out temp-file-then-rename — the same shape the JSON
// stores already use.

fn atomic_write(path: &str, data: &[u8]) -> Result<(), String> {
    let target = Path::new(path);
    let dir = target
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path))?;
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{} has no filename", path))?;

    // Leading dot and a .tmp extension keep the partial file out of both the
    // note index (skips dot-entries) and the watcher (matches .md only).
    let tmp = dir.join(format!(".{}.{}.tmp", name, uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&tmp)
        .map_err(|e| format!("Failed to create {}: {}", tmp.display(), e))?;
    let result = file
        .write_all(data)
        .and_then(|_| fs::rename(&tmp, target))
        .map_err(|e| format!("Failed to replace {}: {}", path, e));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

// ── Self-Write Suppression ────────────────────────────────────────
//
// The watcher cannot tell our own writes from someone editing the file in
// another app, so it re-reads, re-embeds and re-broadcasts every save we make.
// Each write records its path here and the watcher drops the matching event.

const SELF_WRITE_WINDOW: Duration = Duration::from_secs(2);

fn recent_writes() -> &'static Mutex<HashMap<String, Instant>> {
    static RECENT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    RECENT.get_or_init(|| Mutex::new(HashMap::new()))
}

fn record_self_write(path: &str) {
    let mut map = recent_writes().lock().unwrap_or_else(|e| e.into_inner());
    map.retain(|_, at| at.elapsed() < SELF_WRITE_WINDOW);
    map.insert(path.to_string(), Instant::now());
}

/// True if this app wrote `path` within the suppression window. Consumes the
/// record, so a real external edit straight after ours is still seen.
pub fn take_self_write(path: &str) -> bool {
    let mut map = recent_writes().lock().unwrap_or_else(|e| e.into_inner());
    match map.remove(path) {
        Some(at) => at.elapsed() < SELF_WRITE_WINDOW,
        None => false,
    }
}

// ── File Operations ───────────────────────────────────────────────

pub fn read_file(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| e.to_string())
}

pub fn write_file(path: &str, content: &str) -> Result<(), String> {
    record_self_write(path);
    atomic_write(path, content.as_bytes())
}

pub fn write_bytes(path: &str, data: &[u8]) -> Result<(), String> {
    atomic_write(path, data)
}

pub fn delete_file(path: &str) -> Result<(), String> {
    fs::remove_file(path).map_err(|e| e.to_string())
}

/// Rename without replacing another note, directory, or symlink. Fail closed
/// on filesystems without exclusive-rename support instead of check-then-rename.
#[cfg(target_os = "macos")]
pub(crate) fn move_local_path(src: &Path, dst: &Path) -> Result<(), String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn renamex_np(
            src: *const std::ffi::c_char,
            dst: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
    }
    // macOS SDK sys/stdio.h; available since macOS 10.12.
    const RENAME_EXCL: u32 = 0x00000004;
    let src = CString::new(src.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let dst = CString::new(dst.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    // SAFETY: both pointers are valid NUL-terminated strings for this call.
    if unsafe { renamex_np(src.as_ptr(), dst.as_ptr(), RENAME_EXCL) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn move_local_path(_src: &Path, _dst: &Path) -> Result<(), String> {
    Err("Exclusive note moves require macOS".into())
}

pub fn move_file(src: &str, dst: &str) -> Result<(), String> {
    move_local_path(Path::new(src), Path::new(dst))
}

pub fn copy_file(src: &str, dst: &str) -> Result<(), String> {
    fs::copy(src, dst).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn ensure_dir(path: &str) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

pub fn remove_dir_all(path: &str) -> Result<(), String> {
    fs::remove_dir_all(path).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
    pub modified: Option<String>,
}

pub fn list_dir(path: &str) -> Result<Vec<DirEntry>, String> {
    let entries = fs::read_dir(path).map_err(|e| e.to_string())?;
    Ok(entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let metadata = entry.metadata().ok()?;
            let modified = metadata.modified().ok().map(|t| {
                let dt: chrono::DateTime<chrono::Local> = t.into();
                dt.to_rfc3339()
            });
            Some(DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                is_directory: metadata.is_dir(),
                size: metadata.len(),
                modified,
            })
        })
        .collect())
}

pub fn path_exists(path: &str) -> bool {
    PathBuf::from(path).exists()
}

pub fn is_dir(path: &str) -> bool {
    PathBuf::from(path).is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("stix-storage-{label}-{nanos}"));
        fs::create_dir_all(&dir).expect("temp dir should be creatable");
        dir
    }

    #[test]
    fn atomic_write_creates_the_file_and_leaves_no_temp_behind() {
        let dir = unique_temp_dir("create");
        let note = dir.join("20260730-120000-hello-ab12.md");

        atomic_write(note.to_str().unwrap(), b"# hello").expect("write should succeed");

        assert_eq!(fs::read_to_string(&note).unwrap(), "# hello");
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_replaces_existing_content_wholesale() {
        let dir = unique_temp_dir("replace");
        let note = dir.join("note.md");
        fs::write(&note, "a much longer previous body").unwrap();

        atomic_write(note.to_str().unwrap(), b"short").expect("write should succeed");

        // A truncating write that failed midway would leave the tail of the old
        // body; rename cannot.
        assert_eq!(fs::read_to_string(&note).unwrap(), "short");

        let _ = fs::remove_dir_all(&dir);
    }

    /// The property that matters: a concurrent reader never sees a partial note.
    /// A truncating `fs::write` fails this — the reader catches the window
    /// between truncate and the bytes landing, and reads a short or empty file.
    #[test]
    fn a_concurrent_reader_never_sees_a_partial_note() {
        let dir = unique_temp_dir("concurrent");
        let note = dir.join("note.md");
        let long = "x".repeat(64 * 1024);
        let short = "y".repeat(512);
        fs::write(&note, &long).unwrap();

        let reader_path = note.clone();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader_stop = stop.clone();

        let reader = std::thread::spawn(move || {
            let mut partials = 0;
            while !reader_stop.load(std::sync::atomic::Ordering::Relaxed) {
                if let Ok(seen) = fs::read_to_string(&reader_path) {
                    if seen.len() != 64 * 1024 && seen.len() != 512 {
                        partials += 1;
                    }
                }
            }
            partials
        });

        let path_str = note.to_str().unwrap();
        for i in 0..200 {
            let body = if i % 2 == 0 { &short } else { &long };
            atomic_write(path_str, body.as_bytes()).expect("write should succeed");
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);

        let partials = reader.join().expect("reader thread should not panic");
        assert_eq!(partials, 0, "reader saw {partials} partially-written notes");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_rejects_a_path_with_no_filename() {
        assert!(atomic_write("/", b"x").is_err());
    }

    #[test]
    fn concurrent_atomic_writers_each_publish_a_complete_note() {
        let dir = unique_temp_dir("writers");
        let note = dir.join("note.md");
        let barrier = std::sync::Barrier::new(4);
        std::thread::scope(|scope| {
            for byte in b'a'..=b'd' {
                let note = &note;
                let barrier = &barrier;
                scope.spawn(move || {
                    let body = vec![byte; 256 * 1024];
                    barrier.wait();
                    for _ in 0..40 {
                        atomic_write(note.to_str().unwrap(), &body).unwrap();
                    }
                });
            }
        });
        let content = fs::read(&note).unwrap();
        assert_eq!(content.len(), 256 * 1024);
        assert!(content.iter().all(|byte| *byte == content[0]));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn self_write_is_seen_once_then_forgotten() {
        let path = "/tmp/stix-self-write-once.md";
        record_self_write(path);

        assert!(take_self_write(path), "our own write should be suppressed");
        assert!(
            !take_self_write(path),
            "a second event on the same path is an external edit"
        );
    }

    #[test]
    fn a_path_we_never_wrote_is_never_suppressed() {
        assert!(!take_self_write("/tmp/stix-never-written.md"));
    }
}
