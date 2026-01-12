use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use super::exclusions::Exclusions;
use super::metadata::FileAttributes;

/// Represents a single file or directory entry in the scan result
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Path relative to sync root
    pub path: PathBuf,
    /// File size in bytes (0 for directories)
    pub size: u64,
    /// Last modification time
    pub mtime: DateTime<Utc>,
    /// Whether this entry is a directory
    pub is_dir: bool,
    /// SHA-256 hash, computed on demand
    pub hash: Option<String>,
    /// Platform-specific file attributes
    pub attributes: FileAttributes,
}

/// Result of scanning a directory
#[derive(Debug)]
pub struct ScanResult {
    /// Absolute path to the scanned root directory
    pub root: PathBuf,
    /// All entries found during scan
    pub entries: Vec<FileEntry>,
    /// Time when scan was performed
    pub scan_time: DateTime<Utc>,
    /// Paths that were skipped due to errors
    pub skipped: Vec<SkippedEntry>,
}

/// Entry that was skipped during scan
#[derive(Debug)]
pub struct SkippedEntry {
    pub path: PathBuf,
    pub reason: String,
}

/// Directory to skip during scanning
const SKIP_DIR: &str = ".rahzom";

/// Scans a directory and returns structured representation of all files.
///
/// # Arguments
/// * `root` - Path to the directory to scan
///
/// # Returns
/// * `ScanResult` containing all found entries
pub fn scan(root: &Path) -> Result<ScanResult> {
    scan_with_exclusions(root, None)
}

/// Scans a directory with optional exclusion patterns.
///
/// # Arguments
/// * `root` - Path to the directory to scan
/// * `exclusions` - Optional exclusion patterns to filter out matching files
///
/// # Returns
/// * `ScanResult` containing all found entries (excluding filtered files)
pub fn scan_with_exclusions(root: &Path, exclusions: Option<&Exclusions>) -> Result<ScanResult> {
    let root = normalize_path(root)?;
    let mut entries = Vec::new();
    let mut skipped = Vec::new();

    for entry in WalkDir::new(&root).follow_links(false) {
        match entry {
            Ok(entry) => {
                let path = entry.path();

                // Skip the root itself
                if path == root {
                    continue;
                }

                // Skip .rahzom directory and its contents
                if should_skip(path, &root) {
                    continue;
                }

                // Apply exclusion patterns
                if let Some(excl) = exclusions {
                    if let Ok(relative) = path.strip_prefix(&root) {
                        let is_dir = path.is_dir();
                        if excl.is_excluded(relative, is_dir) {
                            skipped.push(SkippedEntry {
                                path: path.to_path_buf(),
                                reason: "Excluded by pattern".to_string(),
                            });
                            continue;
                        }
                    }
                }

                // Skip symlinks (not supported)
                if path.is_symlink() {
                    skipped.push(SkippedEntry {
                        path: path.to_path_buf(),
                        reason: "Symlink (not supported)".to_string(),
                    });
                    continue;
                }

                match process_entry(path, &root) {
                    Ok(file_entry) => entries.push(file_entry),
                    Err(e) => {
                        skipped.push(SkippedEntry {
                            path: path.to_path_buf(),
                            reason: e.to_string(),
                        });
                    }
                }
            }
            Err(e) => {
                let path = e.path().map(|p| p.to_path_buf()).unwrap_or_default();
                skipped.push(SkippedEntry {
                    path,
                    reason: e.to_string(),
                });
            }
        }
    }

    // Sort entries by path for consistent ordering
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(ScanResult {
        root,
        entries,
        scan_time: Utc::now(),
        skipped,
    })
}

/// Computes SHA-256 hash of a file using streaming to avoid loading entire file into memory.
pub fn compute_hash(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("Failed to open file: {:?}", path))?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .with_context(|| format!("Failed to read file: {:?}", path))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Normalizes path for cross-platform compatibility.
/// On Windows, handles long paths by adding \\?\ prefix if needed.
fn normalize_path(path: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path)
        .with_context(|| format!("Failed to canonicalize path: {:?}", path))?;

    #[cfg(windows)]
    {
        // On Windows, canonicalize already returns \\?\ prefixed paths for long paths
        // We keep the canonical path as-is
        Ok(canonical)
    }

    #[cfg(not(windows))]
    {
        Ok(canonical)
    }
}

/// Checks if a path should be skipped during scanning.
fn should_skip(path: &Path, root: &Path) -> bool {
    // Get the relative path from root
    if let Ok(relative) = path.strip_prefix(root) {
        // Check if any component is .rahzom
        for component in relative.components() {
            if let std::path::Component::Normal(name) = component {
                if name == SKIP_DIR {
                    return true;
                }
            }
        }
    }
    false
}

/// Gets platform-specific file attributes from metadata.
#[cfg(windows)]
fn get_file_attributes(metadata: &fs::Metadata) -> FileAttributes {
    use std::os::windows::fs::MetadataExt;
    let attrs = metadata.file_attributes();
    FileAttributes {
        unix_mode: None,
        windows_readonly: Some((attrs & 0x1) != 0),  // FILE_ATTRIBUTE_READONLY
        windows_hidden: Some((attrs & 0x2) != 0),    // FILE_ATTRIBUTE_HIDDEN
    }
}

/// Gets platform-specific file attributes from metadata.
#[cfg(unix)]
fn get_file_attributes(metadata: &fs::Metadata) -> FileAttributes {
    use std::os::unix::fs::PermissionsExt;
    FileAttributes {
        unix_mode: Some(metadata.permissions().mode()),
        windows_readonly: None,
        windows_hidden: None,
    }
}

/// Gets platform-specific file attributes from metadata (fallback for other platforms).
#[cfg(not(any(windows, unix)))]
fn get_file_attributes(_metadata: &fs::Metadata) -> FileAttributes {
    FileAttributes::default()
}

/// Processes a single directory entry into FileEntry.
fn process_entry(path: &Path, root: &Path) -> Result<FileEntry> {
    let metadata =
        fs::metadata(path).with_context(|| format!("Failed to get metadata for: {:?}", path))?;

    let relative_path = path
        .strip_prefix(root)
        .with_context(|| format!("Path {:?} is not under root {:?}", path, root))?
        .to_path_buf();

    let mtime = metadata
        .modified()
        .with_context(|| format!("Failed to get mtime for: {:?}", path))?;

    let mtime_utc = system_time_to_utc(mtime);
    let attributes = get_file_attributes(&metadata);

    Ok(FileEntry {
        path: relative_path,
        size: if metadata.is_dir() { 0 } else { metadata.len() },
        mtime: mtime_utc,
        is_dir: metadata.is_dir(),
        hash: None,
        attributes,
    })
}

/// Converts SystemTime to DateTime<Utc>
fn system_time_to_utc(time: std::time::SystemTime) -> DateTime<Utc> {
    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Utc.timestamp_opt(duration.as_secs() as i64, duration.subsec_nanos())
        .single()
        .unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::exclusions::Exclusions;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        TempDir::new().expect("Failed to create temp directory")
    }

    #[test]
    fn test_scan_empty_directory() {
        let temp = create_test_dir();
        let result = scan(temp.path()).unwrap();

        assert!(result.entries.is_empty());
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn test_scan_flat_directory() {
        let temp = create_test_dir();

        fs::write(temp.path().join("file1.txt"), "content1").unwrap();
        fs::write(temp.path().join("file2.txt"), "content2").unwrap();

        let result = scan(temp.path()).unwrap();

        assert_eq!(result.entries.len(), 2);
        assert!(result
            .entries
            .iter()
            .any(|e| e.path == PathBuf::from("file1.txt")));
        assert!(result
            .entries
            .iter()
            .any(|e| e.path == PathBuf::from("file2.txt")));
    }

    #[test]
    fn test_scan_nested_directory() {
        let temp = create_test_dir();

        fs::create_dir_all(temp.path().join("subdir/nested")).unwrap();
        fs::write(temp.path().join("root.txt"), "root").unwrap();
        fs::write(temp.path().join("subdir/sub.txt"), "sub").unwrap();
        fs::write(temp.path().join("subdir/nested/deep.txt"), "deep").unwrap();

        let result = scan(temp.path()).unwrap();

        // Should have: subdir, subdir/nested, root.txt, subdir/sub.txt, subdir/nested/deep.txt
        assert_eq!(result.entries.len(), 5);

        let dirs: Vec<_> = result.entries.iter().filter(|e| e.is_dir).collect();
        let files: Vec<_> = result.entries.iter().filter(|e| !e.is_dir).collect();

        assert_eq!(dirs.len(), 2);
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn test_scan_skips_rahzom_directory() {
        let temp = create_test_dir();

        fs::create_dir_all(temp.path().join(".rahzom")).unwrap();
        fs::write(temp.path().join(".rahzom/state.json"), "{}").unwrap();
        fs::write(temp.path().join("visible.txt"), "visible").unwrap();

        let result = scan(temp.path()).unwrap();

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].path, PathBuf::from("visible.txt"));
    }

    #[test]
    fn test_file_entry_has_correct_size() {
        let temp = create_test_dir();

        let content = "Hello, World!";
        fs::write(temp.path().join("test.txt"), content).unwrap();

        let result = scan(temp.path()).unwrap();

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].size, content.len() as u64);
    }

    #[test]
    fn test_compute_hash() {
        let temp = create_test_dir();

        let content = "Hello, World!";
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, content).unwrap();

        let hash = compute_hash(&file_path).unwrap();

        // SHA-256 of "Hello, World!" is known
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[test]
    fn test_directories_have_zero_size() {
        let temp = create_test_dir();

        fs::create_dir(temp.path().join("subdir")).unwrap();

        let result = scan(temp.path()).unwrap();

        assert_eq!(result.entries.len(), 1);
        assert!(result.entries[0].is_dir);
        assert_eq!(result.entries[0].size, 0);
    }

    #[test]
    fn test_entries_sorted_by_path() {
        let temp = create_test_dir();

        fs::write(temp.path().join("z.txt"), "z").unwrap();
        fs::write(temp.path().join("a.txt"), "a").unwrap();
        fs::write(temp.path().join("m.txt"), "m").unwrap();

        let result = scan(temp.path()).unwrap();

        let paths: Vec<_> = result.entries.iter().map(|e| &e.path).collect();
        assert_eq!(
            paths,
            vec![
                &PathBuf::from("a.txt"),
                &PathBuf::from("m.txt"),
                &PathBuf::from("z.txt"),
            ]
        );
    }

    #[test]
    fn test_scan_with_exclusions_filters_files() {
        let temp = create_test_dir();

        fs::write(temp.path().join("keep.txt"), "keep").unwrap();
        fs::write(temp.path().join("exclude.tmp"), "exclude").unwrap();
        fs::write(temp.path().join("also_keep.rs"), "code").unwrap();

        let excl = Exclusions::from_patterns(&["*.tmp".to_string()]).unwrap();
        let result = scan_with_exclusions(temp.path(), Some(&excl)).unwrap();

        assert_eq!(result.entries.len(), 2);
        assert!(result.entries.iter().any(|e| e.path == PathBuf::from("keep.txt")));
        assert!(result.entries.iter().any(|e| e.path == PathBuf::from("also_keep.rs")));
        assert!(!result.entries.iter().any(|e| e.path == PathBuf::from("exclude.tmp")));

        // Excluded file should be in skipped list
        assert!(result.skipped.iter().any(|s| s.reason.contains("Excluded")));
    }

    #[test]
    fn test_scan_with_exclusions_filters_directories() {
        let temp = create_test_dir();

        fs::create_dir(temp.path().join("node_modules")).unwrap();
        fs::write(temp.path().join("node_modules/pkg.json"), "{}").unwrap();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/main.rs"), "fn main() {}").unwrap();

        let excl = Exclusions::from_patterns(&["node_modules/".to_string()]).unwrap();
        let result = scan_with_exclusions(temp.path(), Some(&excl)).unwrap();

        // Should only have src and src/main.rs
        assert_eq!(result.entries.len(), 2);
        assert!(result.entries.iter().any(|e| e.path == PathBuf::from("src")));
        assert!(result.entries.iter().any(|e| e.path == PathBuf::from("src/main.rs") || e.path == PathBuf::from("src\\main.rs")));

        // node_modules directory and its contents should not be in entries
        assert!(!result.entries.iter().any(|e| e.path.to_string_lossy().contains("node_modules")));
    }

    #[test]
    fn test_scan_with_no_exclusions_same_as_scan() {
        let temp = create_test_dir();

        fs::write(temp.path().join("file.txt"), "content").unwrap();

        let result1 = scan(temp.path()).unwrap();
        let result2 = scan_with_exclusions(temp.path(), None).unwrap();

        assert_eq!(result1.entries.len(), result2.entries.len());
        assert_eq!(result1.entries[0].path, result2.entries[0].path);
    }

    #[test]
    fn test_scan_with_multiple_exclusion_patterns() {
        let temp = create_test_dir();

        fs::write(temp.path().join("file.txt"), "keep").unwrap();
        fs::write(temp.path().join("file.tmp"), "exclude").unwrap();
        fs::write(temp.path().join("file.log"), "exclude").unwrap();
        fs::write(temp.path().join(".DS_Store"), "exclude").unwrap();

        let excl = Exclusions::from_patterns(&[
            "*.tmp".to_string(),
            "*.log".to_string(),
            ".DS_Store".to_string(),
        ])
        .unwrap();
        let result = scan_with_exclusions(temp.path(), Some(&excl)).unwrap();

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].path, PathBuf::from("file.txt"));
    }

    #[test]
    #[cfg(unix)]
    fn test_scan_skips_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = create_test_dir();

        // Create a regular file
        fs::write(temp.path().join("regular.txt"), "content").unwrap();

        // Create a symlink to the regular file
        symlink(temp.path().join("regular.txt"), temp.path().join("link.txt")).unwrap();

        let result = scan(temp.path()).unwrap();

        // Should only have the regular file
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].path, PathBuf::from("regular.txt"));

        // Symlink should be in skipped list
        assert_eq!(result.skipped.len(), 1);
        assert!(result.skipped[0].reason.contains("Symlink"));
    }

    #[test]
    #[cfg(unix)]
    fn test_scan_skips_broken_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = create_test_dir();

        // Create a regular file
        fs::write(temp.path().join("regular.txt"), "content").unwrap();

        // Create a symlink pointing to a non-existent target
        symlink(temp.path().join("nonexistent.txt"), temp.path().join("broken_link.txt")).unwrap();

        let result = scan(temp.path()).unwrap();

        // Should only have the regular file
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].path, PathBuf::from("regular.txt"));

        // Broken symlink should be in skipped list
        assert_eq!(result.skipped.len(), 1);
        assert!(result.skipped[0].reason.contains("Symlink"));
    }

    #[test]
    #[cfg(windows)]
    fn test_scan_handles_long_paths() {
        let temp = create_test_dir();

        // Create a deeply nested directory structure with path > 260 chars
        // Each segment is 50 chars, with 6 levels that's 300+ chars
        let long_segment = "a".repeat(50);
        let mut deep_path = temp.path().to_path_buf();
        for _ in 0..6 {
            deep_path = deep_path.join(&long_segment);
        }

        // Create the nested directory and a file in it
        fs::create_dir_all(&deep_path).unwrap();
        fs::write(deep_path.join("test.txt"), "content").unwrap();

        // Verify the absolute path is > 260 chars
        let full_path = deep_path.join("test.txt");
        assert!(
            full_path.to_string_lossy().len() > 260,
            "Path should be > 260 chars for this test"
        );

        // Scan should work without error
        let result = scan(temp.path()).unwrap();

        // Should have all the directories and the file
        assert!(!result.entries.is_empty());

        // Should find the file in the deep path
        let has_test_file = result
            .entries
            .iter()
            .any(|e| e.path.to_string_lossy().contains("test.txt"));
        assert!(has_test_file, "Should find test.txt in deeply nested path");
    }
}
