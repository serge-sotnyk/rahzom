use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Directory name for metadata storage
const METADATA_DIR: &str = ".rahzom";
/// State file name
const STATE_FILE: &str = "state.json";
/// Default retention period for deleted files (days)
const DEFAULT_DELETED_RETENTION_DAYS: i64 = 90;

/// File attributes (platform-specific)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FileAttributes {
    /// Unix file mode (permissions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unix_mode: Option<u32>,
    /// Windows read-only attribute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows_readonly: Option<bool>,
    /// Windows hidden attribute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows_hidden: Option<bool>,
}

/// State of a single file as recorded during last sync
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileState {
    /// Relative path from sync root
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Last modification time
    pub mtime: DateTime<Utc>,
    /// SHA-256 hash (if computed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// Platform-specific attributes
    #[serde(default)]
    pub attributes: FileAttributes,
    /// When this file was last synced
    pub last_synced: DateTime<Utc>,
}

/// Record of a deleted file (for conflict detection)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeletedFile {
    /// Relative path from sync root
    pub path: String,
    /// File size before deletion
    pub size: u64,
    /// Last modification time before deletion
    pub mtime: DateTime<Utc>,
    /// SHA-256 hash (if was computed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// When the file was deleted
    pub deleted_at: DateTime<Utc>,
}

/// Complete sync metadata for one side of synchronization
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncMetadata {
    /// Known file states
    pub files: Vec<FileState>,
    /// Recently deleted files (for conflict detection)
    pub deleted: Vec<DeletedFile>,
    /// Timestamp of last successful sync
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<DateTime<Utc>>,
}

impl SyncMetadata {
    /// Creates a new empty metadata
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads metadata from `.rahzom/state.json` in the given directory.
    /// Returns empty metadata if file doesn't exist (fresh start).
    pub fn load(root: &Path) -> Result<Self> {
        Self::load_with_retention(root, DEFAULT_DELETED_RETENTION_DAYS)
    }

    /// Loads metadata with custom retention period for deleted files.
    pub fn load_with_retention(root: &Path, retention_days: i64) -> Result<Self> {
        let state_path = Self::state_file_path(root);

        if !state_path.exists() {
            return Ok(Self::new());
        }

        let file = File::open(&state_path)
            .with_context(|| format!("Failed to open state file: {:?}", state_path))?;

        let reader = BufReader::new(file);

        let mut metadata: SyncMetadata = serde_json::from_reader(reader)
            .with_context(|| format!("Failed to parse state file: {:?}", state_path))?;

        // Cleanup old deleted entries
        metadata.cleanup_deleted(retention_days);

        Ok(metadata)
    }

    /// Saves metadata to `.rahzom/state.json` in the given directory.
    /// Creates `.rahzom/` directory if it doesn't exist.
    pub fn save(&self, root: &Path) -> Result<()> {
        let rahzom_dir = root.join(METADATA_DIR);

        if !rahzom_dir.exists() {
            fs::create_dir_all(&rahzom_dir)
                .with_context(|| format!("Failed to create directory: {:?}", rahzom_dir))?;
        }

        let state_path = Self::state_file_path(root);
        let file = File::create(&state_path)
            .with_context(|| format!("Failed to create state file: {:?}", state_path))?;

        let writer = BufWriter::new(file);

        serde_json::to_writer_pretty(writer, self)
            .with_context(|| format!("Failed to write state file: {:?}", state_path))?;

        Ok(())
    }

    /// Returns path to the state file
    pub fn state_file_path(root: &Path) -> PathBuf {
        root.join(METADATA_DIR).join(STATE_FILE)
    }

    /// Returns path to the .rahzom directory
    pub fn metadata_dir_path(root: &Path) -> PathBuf {
        root.join(METADATA_DIR)
    }

    /// Adds a file to the deleted registry
    pub fn mark_deleted(&mut self, file: DeletedFile) {
        // Remove from files list if present
        self.files.retain(|f| f.path != file.path);
        // Remove old deleted entry for same path if exists
        self.deleted.retain(|d| d.path != file.path);
        // Add to deleted list
        self.deleted.push(file);
    }

    /// Removes entries from deleted list older than retention period
    pub fn cleanup_deleted(&mut self, retention_days: i64) {
        let cutoff = Utc::now() - Duration::days(retention_days);
        self.deleted.retain(|d| d.deleted_at > cutoff);
    }

    /// Finds a file state by path
    pub fn find_file(&self, path: &str) -> Option<&FileState> {
        self.files.iter().find(|f| f.path == path)
    }

    /// Finds a deleted file by path
    pub fn find_deleted(&self, path: &str) -> Option<&DeletedFile> {
        self.deleted.iter().find(|d| d.path == path)
    }

    /// Updates or adds a file state
    pub fn upsert_file(&mut self, file: FileState) {
        // Remove from deleted if was there
        self.deleted.retain(|d| d.path != file.path);

        // Update or add
        if let Some(existing) = self.files.iter_mut().find(|f| f.path == file.path) {
            *existing = file;
        } else {
            self.files.push(file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        TempDir::new().expect("Failed to create temp directory")
    }

    fn sample_file_state(path: &str) -> FileState {
        FileState {
            path: path.to_string(),
            size: 1024,
            mtime: Utc::now(),
            hash: Some("abc123".to_string()),
            attributes: FileAttributes::default(),
            last_synced: Utc::now(),
        }
    }

    fn sample_deleted_file(path: &str) -> DeletedFile {
        DeletedFile {
            path: path.to_string(),
            size: 512,
            mtime: Utc::now(),
            hash: None,
            deleted_at: Utc::now(),
        }
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let temp = create_test_dir();

        let mut metadata = SyncMetadata::new();
        metadata.files.push(sample_file_state("docs/readme.txt"));
        metadata.files.push(sample_file_state("src/main.rs"));
        metadata
            .deleted
            .push(sample_deleted_file("old/removed.txt"));
        metadata.last_sync = Some(Utc::now());

        metadata.save(temp.path()).unwrap();

        let loaded = SyncMetadata::load(temp.path()).unwrap();

        assert_eq!(loaded.files.len(), 2);
        assert_eq!(loaded.deleted.len(), 1);
        assert!(loaded.last_sync.is_some());
        assert_eq!(loaded.files[0].path, "docs/readme.txt");
    }

    #[test]
    fn test_load_nonexistent_returns_empty() {
        let temp = create_test_dir();

        let metadata = SyncMetadata::load(temp.path()).unwrap();

        assert!(metadata.files.is_empty());
        assert!(metadata.deleted.is_empty());
        assert!(metadata.last_sync.is_none());
    }

    #[test]
    fn test_creates_rahzom_directory() {
        let temp = create_test_dir();

        let metadata = SyncMetadata::new();
        metadata.save(temp.path()).unwrap();

        assert!(temp.path().join(".rahzom").exists());
        assert!(temp.path().join(".rahzom/state.json").exists());
    }

    #[test]
    fn test_deleted_files_cleanup() {
        let temp = create_test_dir();

        let mut metadata = SyncMetadata::new();

        // Add recent deleted file
        metadata.deleted.push(sample_deleted_file("recent.txt"));

        // Add old deleted file (100 days ago)
        let mut old_deleted = sample_deleted_file("old.txt");
        old_deleted.deleted_at = Utc::now() - Duration::days(100);
        metadata.deleted.push(old_deleted);

        metadata.save(temp.path()).unwrap();

        // Load with default retention (90 days)
        let loaded = SyncMetadata::load(temp.path()).unwrap();

        // Old file should be cleaned up
        assert_eq!(loaded.deleted.len(), 1);
        assert_eq!(loaded.deleted[0].path, "recent.txt");
    }

    #[test]
    fn test_mark_deleted() {
        let mut metadata = SyncMetadata::new();
        metadata.files.push(sample_file_state("file.txt"));

        let deleted = sample_deleted_file("file.txt");
        metadata.mark_deleted(deleted);

        assert!(metadata.files.is_empty());
        assert_eq!(metadata.deleted.len(), 1);
        assert_eq!(metadata.deleted[0].path, "file.txt");
    }

    #[test]
    fn test_upsert_file() {
        let mut metadata = SyncMetadata::new();

        // Add new file
        metadata.upsert_file(sample_file_state("file.txt"));
        assert_eq!(metadata.files.len(), 1);

        // Update existing file
        let mut updated = sample_file_state("file.txt");
        updated.size = 2048;
        metadata.upsert_file(updated);

        assert_eq!(metadata.files.len(), 1);
        assert_eq!(metadata.files[0].size, 2048);
    }

    #[test]
    fn test_upsert_removes_from_deleted() {
        let mut metadata = SyncMetadata::new();
        metadata.deleted.push(sample_deleted_file("file.txt"));

        metadata.upsert_file(sample_file_state("file.txt"));

        assert!(metadata.deleted.is_empty());
        assert_eq!(metadata.files.len(), 1);
    }

    #[test]
    fn test_find_file() {
        let mut metadata = SyncMetadata::new();
        metadata.files.push(sample_file_state("file.txt"));

        assert!(metadata.find_file("file.txt").is_some());
        assert!(metadata.find_file("other.txt").is_none());
    }

    #[test]
    fn test_corrupted_file_handling() {
        let temp = create_test_dir();

        // Create corrupted state file
        let rahzom_dir = temp.path().join(".rahzom");
        fs::create_dir_all(&rahzom_dir).unwrap();
        fs::write(rahzom_dir.join("state.json"), "{ invalid json }").unwrap();

        let result = SyncMetadata::load(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_retention_period() {
        let temp = create_test_dir();

        let mut metadata = SyncMetadata::new();

        // Add file deleted 10 days ago
        let mut deleted = sample_deleted_file("file.txt");
        deleted.deleted_at = Utc::now() - Duration::days(10);
        metadata.deleted.push(deleted);

        metadata.save(temp.path()).unwrap();

        // Load with 5 day retention - should be cleaned
        let loaded = SyncMetadata::load_with_retention(temp.path(), 5).unwrap();
        assert!(loaded.deleted.is_empty());

        // Load with 15 day retention - should be kept
        let loaded = SyncMetadata::load_with_retention(temp.path(), 15).unwrap();
        assert_eq!(loaded.deleted.len(), 1);
    }
}
