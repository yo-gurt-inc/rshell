use std::fs;
use std::env;
use std::io;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub fn split_dir_prefix(path: &str) -> Option<(String, String)> {
    if let Some(idx) = path.rfind('/') {
        let dir = if idx == 0 {
            "/".to_string()
        } else {
            path[..idx].to_string()
        };
        let prefix = path[idx + 1..].to_string();
        Some((dir, prefix))
    } else {
        None
    }
}

pub fn list_dir_matches(dir: &str, prefix: &str) -> io::Result<Vec<String>> {
    let mut matches = Vec::new();
    let entries = fs::read_dir(dir)?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(prefix) {
            if entry.path().is_dir() {
                matches.push(format!("{}/", name));
            } else {
                matches.push(name);
            }
        }
    }
    matches.sort();
    Ok(matches)
}

pub fn list_path_commands(prefix: &str) -> io::Result<Vec<String>> {
    let mut matches = Vec::new();
    if let Ok(path_var) = env::var("PATH") {
        for dir in path_var.split(':') {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(prefix) {
                        #[cfg(unix)]
                        {
                            if let Ok(meta) = entry.metadata() {
                                if meta.permissions().mode() & 0o111 != 0 {
                                    matches.push(name);
                                }
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            matches.push(name);
                        }
                    }
                }
            }
        }
    }
    matches.sort();
    matches.dedup();
    Ok(matches)
}

pub fn common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = &strings[0];
    let mut prefix_len = first.len();
    for s in &strings[1..] {
        prefix_len = prefix_len.min(
            first
                .chars()
                .zip(s.chars())
                .take_while(|(a, b)| a == b)
                .count(),
        );
    }
    first.chars().take(prefix_len).collect()
}
