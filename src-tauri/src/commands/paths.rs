use std::path::{Component, Path, PathBuf};

fn validate_dev_root(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path.parent().is_none()
        || path.components().any(|part| part == Component::ParentDir)
    {
        return Err("STIX_DEV_ROOT must be an absolute, non-root path without '..'".into());
    }
    Ok(())
}

/// Opt-in development state, never a fallback to the user's regular vault.
pub fn dev_root() -> Result<Option<PathBuf>, String> {
    static ROOT: std::sync::OnceLock<Result<Option<PathBuf>, String>> = std::sync::OnceLock::new();
    ROOT.get_or_init(resolve_dev_root).clone()
}

fn resolve_dev_root() -> Result<Option<PathBuf>, String> {
    let Some(value) = std::env::var_os("STIX_DEV_ROOT") else {
        return Ok(None);
    };
    if !cfg!(debug_assertions) {
        return Err("STIX_DEV_ROOT requires a debug build".into());
    }
    let root = PathBuf::from(value);
    validate_dev_root(&root)?;
    let root = root
        .canonicalize()
        .map_err(|error| format!("Cannot resolve STIX_DEV_ROOT: {error}"))?;
    validate_dev_root(&root)?;
    if !root.is_dir() {
        return Err("STIX_DEV_ROOT must be a directory".into());
    }
    Ok(Some(root))
}

pub fn config_dir() -> Result<PathBuf, String> {
    let directory = match dev_root()? {
        Some(root) => root.join("config"),
        None => {
            let home = dirs::home_dir().ok_or("Could not find home directory")?;
            let current = home.join(".stix");
            // A failed rename must not hide the existing settings.
            match migrate_directory(&home.join(".stix"), &current) {
                Ok(_) => current,
                Err(error) => {
                    eprintln!("{error}");
                    let legacy = home.join(".stix");
                    if legacy.is_dir() {
                        legacy
                    } else {
                        current
                    }
                }
            }
        }
    };
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

fn note_once(message: String) {
    use std::sync::Mutex;
    static NOTES: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut notes = NOTES.lock().unwrap_or_else(|error| error.into_inner());
    if notes.iter().any(|existing| existing == &message) {
        return;
    }
    notes.push(message.clone());
    eprintln!("{message}");
}

/// Move `from` onto `to` when only the old directory exists.
/// Both present means the new tree wins and the old one is left untouched.
pub(crate) fn migrate_directory(from: &Path, to: &Path) -> Result<bool, String> {
    if to.exists() {
        if from.exists() {
            note_once(format!(
                "Both {} and {} exist; keeping {}",
                from.display(),
                to.display(),
                to.display()
            ));
        }
        return Ok(false);
    }
    if !from.exists() {
        return Ok(false);
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    match std::fs::rename(from, to) {
        Ok(()) => {
            eprintln!("Moved {} to {}", from.display(), to.display());
            Ok(true)
        }
        Err(_) if to.exists() => Ok(false),
        Err(error) => Err(format!(
            "Failed to move {} to {}: {error}",
            from.display(),
            to.display()
        )),
    }
}

/// Replace a directory prefix in JSON under `directory`. The next character
/// must be a path separator or the end of a JSON string, so `Stix` does not
/// match `StixExtra`.
pub(crate) fn rewrite_path_prefix(directory: &Path, from: &Path, to: &Path) -> Result<(), String> {
    if !directory.is_dir() {
        return Ok(());
    }
    let from_text = from.to_string_lossy();
    let to_text = to.to_string_lossy();
    rewrite_path_prefix_in(directory, &from_text, &to_text)
}

fn rewrite_path_prefix_in(directory: &Path, from: &str, to: &str) -> Result<(), String> {
    let entries = std::fs::read_dir(directory).map_err(|error| error.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some("fonts") {
                continue;
            }
            rewrite_path_prefix_in(&path, from, to)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let updated = replace_path_prefix(&text, from, to);
        if updated == text {
            continue;
        }
        let temporary = path.with_extension("json.migrating");
        std::fs::write(&temporary, updated).map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn replace_path_prefix(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_string();
    }
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find(from) {
        let next = index + from.len();
        let boundary = rest[next..]
            .chars()
            .next()
            .is_none_or(|character| character == '/' || character == '"' || character == '\\');
        output.push_str(&rest[..index]);
        output.push_str(if boundary { to } else { from });
        rest = &rest[next..];
    }
    output.push_str(rest);
    output
}

/// `preferred` is the Stix location. When it does not exist yet and a sibling
/// `Stix` directory does, that sibling is renamed and saved paths are updated.
pub(crate) fn adopt_legacy_notes_root(
    preferred: &Path,
    config: Option<&Path>,
) -> Result<PathBuf, String> {
    if preferred.file_name().and_then(|name| name.to_str()) != Some("Stix") {
        return Ok(preferred.to_path_buf());
    }
    let Some(parent) = preferred.parent() else {
        return Ok(preferred.to_path_buf());
    };
    let legacy = parent.join("Stix");
    match migrate_directory(&legacy, preferred) {
        Ok(true) => {
            if let Some(config) = config {
                if let Err(error) = rewrite_path_prefix(config, &legacy, preferred) {
                    eprintln!("Moved notes but could not update saved paths: {error}");
                }
            }
            Ok(preferred.to_path_buf())
        }
        Ok(false) => Ok(preferred.to_path_buf()),
        Err(error) => {
            eprintln!("{error}");
            if legacy.is_dir() {
                Ok(legacy)
            } else {
                Ok(preferred.to_path_buf())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_ambiguous_development_roots() {
        for path in ["", ".", "relative", "/", "/tmp/../Users"] {
            assert!(validate_dev_root(Path::new(path)).is_err(), "{path}");
        }
        assert!(validate_dev_root(Path::new("/tmp/stix-qa/session")).is_ok());
    }

    #[test]
    fn moves_a_legacy_directory_only_when_the_new_one_is_absent() {
        let root = std::env::temp_dir().join(format!("stix-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let legacy = root.join("Stix");
        let current = root.join("Stix");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("note.md"), "kept").unwrap();

        assert!(migrate_directory(&legacy, &current).unwrap());
        assert_eq!(
            std::fs::read_to_string(current.join("note.md")).unwrap(),
            "kept"
        );
        assert!(!legacy.exists());
        assert!(!migrate_directory(&legacy, &current).unwrap());

        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("other.md"), "left").unwrap();
        assert!(!migrate_directory(&legacy, &current).unwrap());
        assert!(legacy.join("other.md").is_file());
        assert!(current.join("note.md").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn path_rewrite_stops_at_the_directory_boundary() {
        let text = r#"{"path":"/Users/a/Documents/Stix/Inbox/a.md","other":"/Users/a/Documents/StixExtra"}"#;
        let updated =
            replace_path_prefix(text, "/Users/a/Documents/Stix", "/Users/a/Documents/Stix");
        assert_eq!(
            updated,
            r#"{"path":"/Users/a/Documents/Stix/Inbox/a.md","other":"/Users/a/Documents/StixExtra"}"#
        );
    }

    #[test]
    fn adopting_a_notes_root_rewrites_saved_paths() {
        let root = std::env::temp_dir().join(format!("stix-adopt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let legacy = root.join("Documents").join("Stix");
        let current = root.join("Documents").join("Stix");
        let config = root.join("config");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(legacy.join("note.md"), "note").unwrap();
        std::fs::write(
            config.join("embeddings.json"),
            format!(r#"{{"{}":"vector"}}"#, legacy.join("note.md").display()),
        )
        .unwrap();

        let adopted = adopt_legacy_notes_root(&current, Some(&config)).unwrap();
        assert_eq!(adopted, current);
        assert!(current.join("note.md").is_file());
        let embeddings = std::fs::read_to_string(config.join("embeddings.json")).unwrap();
        assert!(embeddings.contains(&current.join("note.md").to_string_lossy().into_owned()));
        assert!(!embeddings.contains(&legacy.to_string_lossy().into_owned()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
