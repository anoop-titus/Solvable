use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::io;

/// Read .env file into key-value map.
pub fn load(path: &Path) -> HashMap<String, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let mut map = HashMap::new();
    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() == 2 {
            let key = parts[0].trim().to_string();
            let val = parts[1].trim().trim_matches('"').to_string();
            if !key.is_empty() {
                map.insert(key, val);
            }
        }
    }
    map
}

/// Check if .env has at least one credential key.
pub fn has_credentials(path: &Path) -> bool {
    !load(path).is_empty()
}

/// Save key-value pairs into .env, merging with existing content.
/// Preserves comments and keys not in `values`.
pub fn save(path: &Path, values: &HashMap<String, String>) -> io::Result<()> {
    let existing = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };

    let mut lines: Vec<String> = Vec::new();
    let mut written_keys = std::collections::HashSet::new();

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            lines.push(line.to_string());
            continue;
        }
        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim();
            if let Some(new_val) = values.get(key) {
                lines.push(format!("{}=\"{}\"", key, new_val));
                written_keys.insert(key.to_string());
            } else {
                lines.push(line.to_string());
            }
        } else {
            lines.push(line.to_string());
        }
    }

    for (key, val) in values {
        if !written_keys.contains(key.as_str()) {
            lines.push(format!("{}=\"{}\"", key, val));
        }
    }

    let content = lines.join("\n") + "\n";

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true).create(true).truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(content.as_bytes())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, &content)?;
    }
    Ok(())
}

/// Resolve .env path: $SOLVABLE_ENV or CWD/.env
pub fn resolve_env_path() -> PathBuf {
    if let Ok(p) = std::env::var("SOLVABLE_ENV") {
        PathBuf::from(p)
    } else {
        PathBuf::from(".env")
    }
}
