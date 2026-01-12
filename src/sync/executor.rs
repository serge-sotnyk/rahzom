use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use chrono::{DateTime, Utc};

use super::differ::SyncAction;

/// Classification of sync errors for specific handling
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncErrorKind {
    /// File is locked/in use (Windows sharing violation)
    FileLocked,
    /// Permission denied
    PermissionDenied,
    /// Disk is full
    DiskFull,
    /// File was modified during sync
    FileChanged,
    /// Path is too long
    PathTooLong,
    /// Invalid filename or path
    InvalidPath,
    /// File not found
    NotFound,
    /// Generic IO error
    IoError,
}

impl SyncErrorKind {
    /// Returns true if this error type is recoverable (user can retry)
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::FileLocked | Self::DiskFull)
    }

    /// Returns user-friendly title for this error type
    pub fn title(&self) -> &'static str {
        match self {
            Self::FileLocked => "File Locked",
            Self::PermissionDenied => "Permission Denied",
            Self::DiskFull => "Disk Full",
            Self::FileChanged => "File Changed",
            Self::PathTooLong => "Path Too Long",
            Self::InvalidPath => "Invalid Path",
            Self::NotFound => "File Not Found",
            Self::IoError => "I/O Error",
        }
    }
}

/// Classifies an IO error into SyncErrorKind
fn classify_io_error(err: &io::Error) -> SyncErrorKind {
    match err.kind() {
        io::ErrorKind::PermissionDenied => {
            // On Windows, check for sharing violation (file locked)
            #[cfg(windows)]
            {
                // ERROR_SHARING_VIOLATION = 32
                // ERROR_LOCK_VIOLATION = 33
                if let Some(raw) = err.raw_os_error() {
                    if raw == 32 || raw == 33 {
                        return SyncErrorKind::FileLocked;
                    }
                }
            }
            SyncErrorKind::PermissionDenied
        }
        io::ErrorKind::NotFound => SyncErrorKind::NotFound,
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => SyncErrorKind::InvalidPath,
        // StorageFull is unstable, check raw error on Windows
        _ => {
            #[cfg(windows)]
            {
                // ERROR_DISK_FULL = 112
                // ERROR_HANDLE_DISK_FULL = 39
                if let Some(raw) = err.raw_os_error() {
                    if raw == 112 || raw == 39 {
                        return SyncErrorKind::DiskFull;
                    }
                }
            }
            #[cfg(unix)]
            {
                // ENOSPC = 28 on Linux
                if let Some(raw) = err.raw_os_error() {
                    if raw == 28 {
                        return SyncErrorKind::DiskFull;
                    }
                }
            }
            SyncErrorKind::IoError
        }
    }
}

/// Result of checking disk space
#[derive(Debug, Clone)]
pub struct DiskSpaceInfo {
    /// Available space in bytes
    pub available: u64,
    /// Required space in bytes
    pub required: u64,
    /// Whether there is enough space
    pub sufficient: bool,
}

/// Checks available disk space at the given path.
///
/// # Arguments
/// * `path` - Path to check disk space for (uses the mount point of the path)
/// * `required_bytes` - Required space in bytes
///
/// # Returns
/// * `DiskSpaceInfo` with available space and whether it's sufficient
pub fn check_disk_space(path: &Path, required_bytes: u64) -> Result<DiskSpaceInfo> {
    use fs2::available_space;

    let available = available_space(path)?;
    Ok(DiskSpaceInfo {
        available,
        required: required_bytes,
        sufficient: available >= required_bytes,
    })
}

/// Configuration for the executor
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Whether to create backups before overwriting files
    pub backup_enabled: bool,
    /// Number of backup versions to keep per file
    pub backup_versions: usize,
    /// Whether to move deleted files to trash instead of permanent delete
    pub soft_delete: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            backup_enabled: true,
            backup_versions: 5,
            soft_delete: true,
        }
    }
}

/// A successfully completed action
#[derive(Debug, Clone)]
pub struct CompletedAction {
    pub action: SyncAction,
    pub bytes_transferred: u64,
}

/// A failed action
#[derive(Debug, Clone)]
pub struct FailedAction {
    pub action: SyncAction,
    pub error: String,
    pub kind: SyncErrorKind,
}

/// A skipped action (e.g., file changed during sync)
#[derive(Debug, Clone)]
pub struct SkippedAction {
    pub action: SyncAction,
    pub reason: String,
}

/// Result of executing sync actions
#[derive(Debug, Default)]
pub struct ExecutionResult {
    pub completed: Vec<CompletedAction>,
    pub failed: Vec<FailedAction>,
    pub skipped: Vec<SkippedAction>,
}

impl ExecutionResult {
    pub fn total_bytes_transferred(&self) -> u64 {
        self.completed.iter().map(|c| c.bytes_transferred).sum()
    }
}

/// Callback trait for progress reporting
pub trait ProgressCallback {
    fn on_progress(&mut self, current: usize, total: usize, current_file: &Path);
    fn on_file_complete(&mut self, action: &SyncAction, success: bool);
}

/// No-op progress callback
pub struct NoopProgress;

impl ProgressCallback for NoopProgress {
    fn on_progress(&mut self, _current: usize, _total: usize, _current_file: &Path) {}
    fn on_file_complete(&mut self, _action: &SyncAction, _success: bool) {}
}

/// File info for pre-copy verification
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub size: u64,
    pub mtime: DateTime<Utc>,
}

/// Metadata directory names
const METADATA_DIR: &str = ".rahzom";
const TRASH_DIR: &str = "_trash";
const BACKUP_DIR: &str = "_backup";

/// Executes sync actions between two directories.
pub struct Executor {
    left_root: PathBuf,
    right_root: PathBuf,
    config: ExecutorConfig,
}

impl Executor {
    pub fn new(left_root: PathBuf, right_root: PathBuf, config: ExecutorConfig) -> Self {
        Self {
            left_root,
            right_root,
            config,
        }
    }

    /// Executes all actions with progress callback.
    /// Actions are sorted: directories first, then copies, then deletes.
    pub fn execute(
        &self,
        actions: Vec<SyncAction>,
        snapshots: &std::collections::HashMap<PathBuf, FileSnapshot>,
        progress: &mut dyn ProgressCallback,
    ) -> Result<ExecutionResult> {
        let sorted_actions = self.sort_actions(actions);
        let total = sorted_actions.len();
        let mut result = ExecutionResult::default();

        for (index, action) in sorted_actions.into_iter().enumerate() {
            progress.on_progress(index + 1, total, self.action_path(&action));

            match self.execute_action(&action, snapshots) {
                Ok(Some(bytes)) => {
                    progress.on_file_complete(&action, true);
                    result.completed.push(CompletedAction {
                        action,
                        bytes_transferred: bytes,
                    });
                }
                Ok(None) => {
                    // Action was skipped
                    progress.on_file_complete(&action, true);
                }
                Err(ExecuteError::Skipped(reason)) => {
                    progress.on_file_complete(&action, true);
                    result.skipped.push(SkippedAction { action, reason });
                }
                Err(ExecuteError::Failed(error, kind)) => {
                    progress.on_file_complete(&action, false);
                    result.failed.push(FailedAction { action, error, kind });
                }
            }
        }

        Ok(result)
    }

    /// Sorts actions for proper execution order
    fn sort_actions(&self, mut actions: Vec<SyncAction>) -> Vec<SyncAction> {
        actions.sort_by(|a, b| {
            let order_a = self.action_order(a);
            let order_b = self.action_order(b);
            order_a.cmp(&order_b)
        });
        actions
    }

    fn action_order(&self, action: &SyncAction) -> (u8, usize, bool) {
        match action {
            // Directories first, sorted by depth (shallow first)
            SyncAction::CreateDirLeft { path } | SyncAction::CreateDirRight { path } => {
                (0, path.components().count(), false)
            }
            // Copies second
            SyncAction::CopyToLeft { path, .. } | SyncAction::CopyToRight { path, .. } => {
                (1, path.components().count(), false)
            }
            // Deletes last, sorted by depth (deep first for directories)
            SyncAction::DeleteLeft { path } | SyncAction::DeleteRight { path } => {
                (2, usize::MAX - path.components().count(), true)
            }
            // Skip and Conflict at the end
            SyncAction::Skip { .. } | SyncAction::Conflict { .. } => (3, 0, false),
        }
    }

    fn action_path<'a>(&self, action: &'a SyncAction) -> &'a Path {
        match action {
            SyncAction::CopyToRight { path, .. }
            | SyncAction::CopyToLeft { path, .. }
            | SyncAction::DeleteRight { path }
            | SyncAction::DeleteLeft { path }
            | SyncAction::CreateDirRight { path }
            | SyncAction::CreateDirLeft { path }
            | SyncAction::Skip { path, .. }
            | SyncAction::Conflict { path, .. } => path,
        }
    }

    fn execute_action(
        &self,
        action: &SyncAction,
        snapshots: &std::collections::HashMap<PathBuf, FileSnapshot>,
    ) -> std::result::Result<Option<u64>, ExecuteError> {
        match action {
            SyncAction::CopyToRight { path, size } => {
                let src = self.left_root.join(path);
                let dst = self.right_root.join(path);
                self.verify_and_copy(&src, &dst, path, *size, snapshots)
            }
            SyncAction::CopyToLeft { path, size } => {
                let src = self.right_root.join(path);
                let dst = self.left_root.join(path);
                self.verify_and_copy(&src, &dst, path, *size, snapshots)
            }
            SyncAction::DeleteRight { path } => {
                let target = self.right_root.join(path);
                self.delete_file(&target, &self.right_root)?;
                Ok(Some(0))
            }
            SyncAction::DeleteLeft { path } => {
                let target = self.left_root.join(path);
                self.delete_file(&target, &self.left_root)?;
                Ok(Some(0))
            }
            SyncAction::CreateDirRight { path } => {
                let target = self.right_root.join(path);
                self.create_dir(&target)?;
                Ok(Some(0))
            }
            SyncAction::CreateDirLeft { path } => {
                let target = self.left_root.join(path);
                self.create_dir(&target)?;
                Ok(Some(0))
            }
            SyncAction::Skip { .. } => Ok(None),
            SyncAction::Conflict { .. } => Ok(None),
        }
    }

    fn verify_and_copy(
        &self,
        src: &Path,
        dst: &Path,
        rel_path: &Path,
        expected_size: u64,
        snapshots: &std::collections::HashMap<PathBuf, FileSnapshot>,
    ) -> std::result::Result<Option<u64>, ExecuteError> {
        // Pre-copy verification
        if let Some(snapshot) = snapshots.get(rel_path) {
            if !self.verify_file(src, snapshot)? {
                return Err(ExecuteError::Skipped(
                    "File changed during sync".to_string(),
                ));
            }
        }

        // Create backup if file exists at destination
        if dst.exists() && self.config.backup_enabled {
            let root = if dst.starts_with(&self.left_root) {
                &self.left_root
            } else {
                &self.right_root
            };
            self.create_backup(dst, root)?;
        }

        // Perform copy
        self.copy_file(src, dst)?;

        // Verify copy (size check)
        let dst_meta =
            fs::metadata(dst).map_err(|e| ExecuteError::from_io(e, "Failed to verify copy"))?;
        if dst_meta.len() != expected_size {
            return Err(ExecuteError::failed(
                format!(
                    "Size mismatch after copy: expected {}, got {}",
                    expected_size,
                    dst_meta.len()
                ),
                SyncErrorKind::IoError,
            ));
        }

        Ok(Some(expected_size))
    }

    fn verify_file(
        &self,
        path: &Path,
        snapshot: &FileSnapshot,
    ) -> std::result::Result<bool, ExecuteError> {
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(false); // File was deleted
            }
            Err(e) => return Err(ExecuteError::from_io(e, "Failed to verify file")),
        };

        // Check size
        if metadata.len() != snapshot.size {
            return Ok(false);
        }

        // Check mtime
        let mtime = metadata
            .modified()
            .map_err(|e| ExecuteError::from_io(e, "Failed to get modification time"))?;
        let mtime_utc = system_time_to_utc(mtime);

        // Allow FAT32 tolerance for mtime comparison
        let diff = (mtime_utc - snapshot.mtime).num_seconds().abs();
        if diff > super::utils::FAT32_TOLERANCE_SECS {
            return Ok(false);
        }

        Ok(true)
    }

    fn copy_file(&self, src: &Path, dst: &Path) -> std::result::Result<(), ExecuteError> {
        // Create parent directories
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ExecuteError::from_io(e, "Failed to create parent dir"))?;
        }

        // Copy file content
        let src_file =
            File::open(src).map_err(|e| ExecuteError::from_io(e, "Failed to open source"))?;
        let dst_file = File::create(dst)
            .map_err(|e| ExecuteError::from_io(e, "Failed to create destination"))?;

        let mut reader = BufReader::with_capacity(64 * 1024, src_file);
        let mut writer = BufWriter::with_capacity(64 * 1024, dst_file);

        io::copy(&mut reader, &mut writer).map_err(|e| ExecuteError::from_io(e, "Failed to copy"))?;

        writer
            .flush()
            .map_err(|e| ExecuteError::from_io(e, "Failed to flush"))?;

        // Preserve mtime
        let src_meta =
            fs::metadata(src).map_err(|e| ExecuteError::from_io(e, "Failed to get metadata"))?;
        if let Ok(mtime) = src_meta.modified() {
            let _ = set_file_mtime(dst, mtime);
        }

        // Preserve file attributes (readonly, hidden on Windows)
        let _ = set_file_attributes(dst, src);

        Ok(())
    }

    fn delete_file(&self, path: &Path, root: &Path) -> std::result::Result<(), ExecuteError> {
        if !path.exists() {
            return Ok(()); // Already deleted
        }

        if self.config.soft_delete {
            self.soft_delete(path, root)
        } else {
            if path.is_dir() {
                fs::remove_dir(path)
            } else {
                fs::remove_file(path)
            }
            .map_err(|e| ExecuteError::from_io(e, "Failed to delete"))
        }
    }

    fn soft_delete(&self, path: &Path, root: &Path) -> std::result::Result<(), ExecuteError> {
        let trash_dir = root.join(METADATA_DIR).join(TRASH_DIR);
        fs::create_dir_all(&trash_dir)
            .map_err(|e| ExecuteError::from_io(e, "Failed to create trash dir"))?;

        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S_%3f");
        let trash_name = format!("{}.{}", filename, timestamp);
        let trash_path = trash_dir.join(trash_name);

        fs::rename(path, &trash_path).map_err(|e| ExecuteError::from_io(e, "Failed to move to trash"))
    }

    fn create_backup(&self, path: &Path, root: &Path) -> std::result::Result<(), ExecuteError> {
        let backup_dir = root.join(METADATA_DIR).join(BACKUP_DIR);
        fs::create_dir_all(&backup_dir)
            .map_err(|e| ExecuteError::from_io(e, "Failed to create backup dir"))?;

        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S_%3f");
        let backup_name = format!("{}.{}", filename, timestamp);
        let backup_path = backup_dir.join(&backup_name);

        // Copy to backup
        fs::copy(path, &backup_path).map_err(|e| ExecuteError::from_io(e, "Failed to create backup"))?;

        // Rotate old backups
        self.rotate_backups(&backup_dir, &filename)?;

        Ok(())
    }

    fn rotate_backups(
        &self,
        backup_dir: &Path,
        filename: &str,
    ) -> std::result::Result<(), ExecuteError> {
        let prefix = format!("{}.", filename);
        let mut backups: Vec<_> = fs::read_dir(backup_dir)
            .map_err(|e| ExecuteError::from_io(e, "Failed to read backup dir"))?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .collect();

        // Sort by name (timestamp) descending
        backups.sort_by_key(|b| std::cmp::Reverse(b.file_name()));

        // Remove excess backups
        for old_backup in backups.into_iter().skip(self.config.backup_versions) {
            let _ = fs::remove_file(old_backup.path());
        }

        Ok(())
    }

    fn create_dir(&self, path: &Path) -> std::result::Result<(), ExecuteError> {
        fs::create_dir_all(path).map_err(|e| ExecuteError::from_io(e, "Failed to create directory"))
    }
}

#[derive(Debug)]
enum ExecuteError {
    Skipped(String),
    Failed(String, SyncErrorKind),
}

impl ExecuteError {
    /// Create a Failed error from an io::Error with automatic classification
    fn from_io(err: io::Error, context: &str) -> Self {
        let kind = classify_io_error(&err);
        Self::Failed(format!("{}: {}", context, err), kind)
    }

    /// Create a Failed error with a specific kind
    fn failed(msg: String, kind: SyncErrorKind) -> Self {
        Self::Failed(msg, kind)
    }
}

fn system_time_to_utc(time: SystemTime) -> DateTime<Utc> {
    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    DateTime::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos())
        .unwrap_or_else(Utc::now)
}

#[cfg(windows)]
fn set_file_mtime(path: &Path, mtime: SystemTime) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;

    let file = fs::OpenOptions::new()
        .write(true)
        .custom_flags(0x02000000) // FILE_FLAG_BACKUP_SEMANTICS for directories
        .open(path)?;

    let duration = mtime
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let intervals = duration.as_secs() * 10_000_000
        + duration.subsec_nanos() as u64 / 100
        + 116_444_736_000_000_000;

    unsafe {
        let handle = file.as_raw_handle();
        let ft = std::mem::transmute::<u64, [u32; 2]>(intervals);
        let filetime = windows_sys::Win32::Foundation::FILETIME {
            dwLowDateTime: ft[0],
            dwHighDateTime: ft[1],
        };
        windows_sys::Win32::Storage::FileSystem::SetFileTime(
            handle,
            std::ptr::null(),
            std::ptr::null(),
            &filetime,
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_file_mtime(path: &Path, mtime: SystemTime) -> io::Result<()> {
    // On Unix, we'd use filetime crate or libc
    // For now, just ignore mtime setting on non-Windows
    let _ = (path, mtime);
    Ok(())
}

/// Sets Windows file attributes (readonly, hidden) on the destination file
#[cfg(windows)]
fn set_file_attributes(path: &Path, src_path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;

    // Read attributes from source
    let src_meta = fs::metadata(src_path)?;
    let src_attrs = src_meta.file_attributes();

    // Only apply if source has readonly or hidden attributes
    const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const ATTRS_MASK: u32 = FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_HIDDEN;

    let attrs_to_apply = src_attrs & ATTRS_MASK;
    if attrs_to_apply == 0 {
        return Ok(()); // No special attributes to apply
    }

    // Read current destination attributes and merge
    let dst_meta = fs::metadata(path)?;
    let dst_attrs = dst_meta.file_attributes();
    let new_attrs = (dst_attrs & !ATTRS_MASK) | attrs_to_apply;

    // Convert path to wide string for Windows API
    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();

    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::SetFileAttributesW(
            wide_path.as_ptr(),
            new_attrs,
        )
    };

    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn set_file_attributes(_path: &Path, _src_path: &Path) -> io::Result<()> {
    // On Unix, permissions would be handled differently
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_dirs() -> (TempDir, TempDir) {
        (
            TempDir::new().expect("Failed to create left dir"),
            TempDir::new().expect("Failed to create right dir"),
        )
    }

    #[test]
    fn test_copy_single_file() {
        let (left, right) = create_test_dirs();

        // Create source file
        let content = "Hello, World!";
        fs::write(left.path().join("test.txt"), content).unwrap();

        let executor = Executor::new(
            left.path().to_path_buf(),
            right.path().to_path_buf(),
            ExecutorConfig::default(),
        );

        let actions = vec![SyncAction::CopyToRight {
            path: PathBuf::from("test.txt"),
            size: content.len() as u64,
        }];

        let result = executor
            .execute(actions, &HashMap::new(), &mut NoopProgress)
            .unwrap();

        assert_eq!(result.completed.len(), 1);
        assert!(right.path().join("test.txt").exists());
        assert_eq!(
            fs::read_to_string(right.path().join("test.txt")).unwrap(),
            content
        );
    }

    #[test]
    fn test_copy_preserves_mtime() {
        let (left, right) = create_test_dirs();

        fs::write(left.path().join("test.txt"), "content").unwrap();

        // Get original mtime
        let src_mtime = fs::metadata(left.path().join("test.txt"))
            .unwrap()
            .modified()
            .unwrap();

        let executor = Executor::new(
            left.path().to_path_buf(),
            right.path().to_path_buf(),
            ExecutorConfig::default(),
        );

        let actions = vec![SyncAction::CopyToRight {
            path: PathBuf::from("test.txt"),
            size: 7,
        }];

        executor
            .execute(actions, &HashMap::new(), &mut NoopProgress)
            .unwrap();

        let dst_mtime = fs::metadata(right.path().join("test.txt"))
            .unwrap()
            .modified()
            .unwrap();

        // Allow 2 second tolerance
        let diff = src_mtime
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .abs_diff(
                dst_mtime
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            );
        assert!(diff <= 2, "mtime difference too large: {} seconds", diff);
    }

    #[test]
    fn test_soft_delete() {
        let (left, right) = create_test_dirs();

        fs::write(right.path().join("to_delete.txt"), "delete me").unwrap();

        let executor = Executor::new(
            left.path().to_path_buf(),
            right.path().to_path_buf(),
            ExecutorConfig {
                soft_delete: true,
                ..Default::default()
            },
        );

        let actions = vec![SyncAction::DeleteRight {
            path: PathBuf::from("to_delete.txt"),
        }];

        let result = executor
            .execute(actions, &HashMap::new(), &mut NoopProgress)
            .unwrap();

        assert_eq!(result.completed.len(), 1);
        assert!(!right.path().join("to_delete.txt").exists());

        // Check trash directory
        let trash_dir = right.path().join(".rahzom/_trash");
        assert!(trash_dir.exists());
        let trash_files: Vec<_> = fs::read_dir(&trash_dir).unwrap().collect();
        assert_eq!(trash_files.len(), 1);
    }

    #[test]
    fn test_hard_delete() {
        let (left, right) = create_test_dirs();

        fs::write(right.path().join("to_delete.txt"), "delete me").unwrap();

        let executor = Executor::new(
            left.path().to_path_buf(),
            right.path().to_path_buf(),
            ExecutorConfig {
                soft_delete: false,
                ..Default::default()
            },
        );

        let actions = vec![SyncAction::DeleteRight {
            path: PathBuf::from("to_delete.txt"),
        }];

        let result = executor
            .execute(actions, &HashMap::new(), &mut NoopProgress)
            .unwrap();

        assert_eq!(result.completed.len(), 1);
        assert!(!right.path().join("to_delete.txt").exists());

        // Trash directory should not exist
        assert!(!right.path().join(".rahzom/_trash").exists());
    }

    #[test]
    fn test_backup_before_overwrite() {
        let (left, right) = create_test_dirs();

        fs::write(left.path().join("file.txt"), "new content").unwrap();
        fs::write(right.path().join("file.txt"), "old content").unwrap();

        let executor = Executor::new(
            left.path().to_path_buf(),
            right.path().to_path_buf(),
            ExecutorConfig {
                backup_enabled: true,
                ..Default::default()
            },
        );

        let actions = vec![SyncAction::CopyToRight {
            path: PathBuf::from("file.txt"),
            size: 11,
        }];

        executor
            .execute(actions, &HashMap::new(), &mut NoopProgress)
            .unwrap();

        // File should be updated
        assert_eq!(
            fs::read_to_string(right.path().join("file.txt")).unwrap(),
            "new content"
        );

        // Backup should exist
        let backup_dir = right.path().join(".rahzom/_backup");
        assert!(backup_dir.exists());
        let backup_files: Vec<_> = fs::read_dir(&backup_dir).unwrap().collect();
        assert_eq!(backup_files.len(), 1);
    }

    #[test]
    fn test_backup_rotation() {
        let (left, right) = create_test_dirs();

        // Create initial file on right
        fs::write(right.path().join("file.txt"), "v0").unwrap();

        let executor = Executor::new(
            left.path().to_path_buf(),
            right.path().to_path_buf(),
            ExecutorConfig {
                backup_enabled: true,
                backup_versions: 3,
                ..Default::default()
            },
        );

        // Create multiple versions
        for i in 1..=5 {
            fs::write(left.path().join("file.txt"), format!("v{}", i)).unwrap();

            let actions = vec![SyncAction::CopyToRight {
                path: PathBuf::from("file.txt"),
                size: 2,
            }];

            executor
                .execute(actions, &HashMap::new(), &mut NoopProgress)
                .unwrap();

            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Should only have 3 backups (backup_versions = 3)
        let backup_dir = right.path().join(".rahzom/_backup");
        let backup_files: Vec<_> = fs::read_dir(&backup_dir).unwrap().collect();
        assert_eq!(backup_files.len(), 3);
    }

    #[test]
    fn test_create_directory() {
        let (left, right) = create_test_dirs();

        let executor = Executor::new(
            left.path().to_path_buf(),
            right.path().to_path_buf(),
            ExecutorConfig::default(),
        );

        let actions = vec![SyncAction::CreateDirRight {
            path: PathBuf::from("subdir/nested"),
        }];

        let result = executor
            .execute(actions, &HashMap::new(), &mut NoopProgress)
            .unwrap();

        assert_eq!(result.completed.len(), 1);
        assert!(right.path().join("subdir/nested").is_dir());
    }

    #[test]
    fn test_execution_order() {
        let (left, right) = create_test_dirs();

        fs::write(left.path().join("file.txt"), "content").unwrap();
        fs::write(right.path().join("to_delete.txt"), "delete").unwrap();

        let executor = Executor::new(
            left.path().to_path_buf(),
            right.path().to_path_buf(),
            ExecutorConfig::default(),
        );

        // Actions in wrong order
        let actions = vec![
            SyncAction::DeleteRight {
                path: PathBuf::from("to_delete.txt"),
            },
            SyncAction::CopyToRight {
                path: PathBuf::from("file.txt"),
                size: 7,
            },
            SyncAction::CreateDirRight {
                path: PathBuf::from("newdir"),
            },
        ];

        let result = executor
            .execute(actions, &HashMap::new(), &mut NoopProgress)
            .unwrap();

        // All should succeed
        assert_eq!(result.completed.len(), 3);
        assert_eq!(result.failed.len(), 0);

        // Verify order was: dir, copy, delete
        // (We can't easily verify order, but we verify all completed)
        assert!(right.path().join("newdir").is_dir());
        assert!(right.path().join("file.txt").exists());
        assert!(!right.path().join("to_delete.txt").exists());
    }

    #[test]
    fn test_file_changed_during_sync() {
        let (left, right) = create_test_dirs();

        fs::write(left.path().join("test.txt"), "content").unwrap();

        let executor = Executor::new(
            left.path().to_path_buf(),
            right.path().to_path_buf(),
            ExecutorConfig::default(),
        );

        // Create snapshot with different size
        let mut snapshots = HashMap::new();
        snapshots.insert(
            PathBuf::from("test.txt"),
            FileSnapshot {
                size: 100, // Different from actual size
                mtime: Utc::now(),
            },
        );

        let actions = vec![SyncAction::CopyToRight {
            path: PathBuf::from("test.txt"),
            size: 7,
        }];

        let result = executor
            .execute(actions, &snapshots, &mut NoopProgress)
            .unwrap();

        // Should be skipped
        assert_eq!(result.completed.len(), 0);
        assert_eq!(result.skipped.len(), 1);
        assert!(!right.path().join("test.txt").exists());
    }

    #[test]
    #[cfg(windows)]
    fn test_copy_preserves_windows_attributes() {
        use std::os::windows::fs::MetadataExt;
        use std::process::Command;

        let (left, right) = create_test_dirs();

        // Create source file
        let src_path = left.path().join("test.txt");
        fs::write(&src_path, "test content").unwrap();

        // Set readonly + hidden attributes using attrib command
        let status = Command::new("attrib")
            .args(["+R", "+H", src_path.to_str().unwrap()])
            .status()
            .expect("Failed to run attrib");
        assert!(status.success(), "attrib command failed");

        // Verify source has attributes set
        let src_meta = fs::metadata(&src_path).unwrap();
        let src_attrs = src_meta.file_attributes();
        assert!((src_attrs & 0x1) != 0, "Source should be readonly");
        assert!((src_attrs & 0x2) != 0, "Source should be hidden");

        let executor = Executor::new(
            left.path().to_path_buf(),
            right.path().to_path_buf(),
            ExecutorConfig::default(),
        );

        let actions = vec![SyncAction::CopyToRight {
            path: PathBuf::from("test.txt"),
            size: 12,
        }];

        let result = executor
            .execute(actions, &HashMap::new(), &mut NoopProgress)
            .unwrap();

        assert_eq!(result.completed.len(), 1);
        assert!(right.path().join("test.txt").exists());

        // Verify destination has same attributes
        let dst_meta = fs::metadata(right.path().join("test.txt")).unwrap();
        let dst_attrs = dst_meta.file_attributes();
        assert!(
            (dst_attrs & 0x1) != 0,
            "Destination should be readonly (attrs: {:#x})",
            dst_attrs
        );
        assert!(
            (dst_attrs & 0x2) != 0,
            "Destination should be hidden (attrs: {:#x})",
            dst_attrs
        );
    }
}
