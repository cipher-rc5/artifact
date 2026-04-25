// file: src/utils.rs
// description: Utility functions for Space Cleaner

use std::path::{Path, PathBuf};

pub fn get_home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

pub fn delete_directory(path: &Path) -> std::io::Result<()> {
    std::fs::remove_dir_all(path)
}

pub fn format_size(bytes: u64) -> String {
    humansize::format_size(bytes, humansize::BINARY)
}

pub fn format_elapsed(secs: f64) -> String {
    if secs < 60.0 {
        format!("{:.0}s", secs)
    } else {
        format!("{}m {:.0}s", (secs / 60.0) as u64, secs % 60.0)
    }
}

/// List visible subdirectories of `path`, sorted alphabetically.
/// Returns `(name, full_path)` pairs. Hidden directories (starting with `.`) are excluded.
pub fn list_directories(path: &Path) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        if let Ok(ft) = entry.file_type()
            && ft.is_dir()
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                entries.push((name, entry.path()));
            }
        }
    }
    entries.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    Ok(entries)
}
