//! ZIP extraction to a target directory.
//!
//! Archives from Packagist typically have a single top-level directory
//! (e.g. `acme-foo-1.2.3/src/...`). We strip that prefix when extracting
//! to `vendor/acme/foo/` so the files land directly in the package dir.

#![allow(clippy::all)]

use std::io;
use std::path::Path;

/// Extract a ZIP archive into `target_dir`.
///
/// If all entries share a common top-level directory, it is stripped
/// (so `acme-foo-1.2.3/src/Foo.php` → `target_dir/src/Foo.php`).
pub fn extract_zip(bytes: &[u8], target_dir: &Path) -> io::Result<()> {
    let cursor = io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    // First pass: detect common top-level prefix
    let common_prefix = detect_common_prefix(&mut archive)?;

    std::fs::create_dir_all(target_dir)?;

    // Second pass: extract
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let name = entry.name().to_string();

        // Skip the common prefix directory entry itself
        let entry_path = strip_prefix(&name, &common_prefix);
        if entry_path.is_empty() || entry_path == "/" {
            continue;
        }

        let dest = target_dir.join(&entry_path);

        if entry.is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::File::create(&dest)?;
            io::copy(&mut entry, &mut file)?;
        }
    }

    Ok(())
}

/// Detect the common top-level directory name in the archive.
/// Returns `""` if there isn't one.
fn detect_common_prefix(archive: &mut zip::ZipArchive<io::Cursor<&[u8]>>) -> io::Result<String> {
    let mut prefix: Option<String> = None;
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let name = entry.name();
        let top = match name.find('/') {
            Some(idx) => &name[..=idx],
            None => return Ok(String::new()), // no common prefix
        };
        match &prefix {
            None => prefix = Some(top.to_string()),
            Some(p) if p != top => return Ok(String::new()),
            _ => {}
        }
    }
    Ok(prefix.unwrap_or_default())
}

/// Strip `prefix` from the start of `path`.
fn strip_prefix(path: &str, prefix: &str) -> String {
    if !prefix.is_empty() && path.starts_with(prefix) {
        path[prefix.len()..].to_string()
    } else {
        path.to_string()
    }
}
