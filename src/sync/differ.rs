use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use super::metadata::SyncMetadata;
use super::scanner::ScanResult;
use super::utils::FAT32_TOLERANCE_SECS;

/// Information about a file for conflict reporting
#[derive(Debug, Clone, PartialEq)]
pub struct FileInfo {
    pub size: u64,
    pub mtime: DateTime<Utc>,
    pub hash: Option<String>,
}

/// Reason for a sync conflict
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictReason {
    /// Both sides were modified since last sync
    BothModified,
    /// One side modified, other side deleted
    ModifiedAndDeleted,
    /// File exists on one side, was deleted on the other (first sync scenario)
    ExistsVsDeleted,
    /// Files with same name but different case (e.g., File.txt vs file.txt)
    CaseConflict,
}

/// Action to perform during synchronization
#[derive(Debug, Clone, PartialEq)]
pub enum SyncAction {
    /// Copy file from left to right
    CopyToRight { path: PathBuf, size: u64 },
    /// Copy file from right to left
    CopyToLeft { path: PathBuf, size: u64 },
    /// Delete file on right side
    DeleteRight { path: PathBuf },
    /// Delete file on left side
    DeleteLeft { path: PathBuf },
    /// Create directory on right side
    CreateDirRight { path: PathBuf },
    /// Create directory on left side
    CreateDirLeft { path: PathBuf },
    /// Conflict that needs user resolution
    Conflict {
        path: PathBuf,
        reason: ConflictReason,
        left: Option<FileInfo>,
        right: Option<FileInfo>,
    },
    /// Skip this file (no action needed)
    Skip { path: PathBuf, reason: String },
}

impl SyncAction {
    /// Returns the path associated with this action
    pub fn path(&self) -> &PathBuf {
        match self {
            Self::CopyToRight { path, .. } => path,
            Self::CopyToLeft { path, .. } => path,
            Self::DeleteRight { path } => path,
            Self::DeleteLeft { path } => path,
            Self::CreateDirRight { path } => path,
            Self::CreateDirLeft { path } => path,
            Self::Conflict { path, .. } => path,
            Self::Skip { path, .. } => path,
        }
    }
}

/// Result of comparing two sides
#[derive(Debug, Default)]
pub struct DiffResult {
    /// List of actions to perform
    pub actions: Vec<SyncAction>,
    /// Total bytes that need to be transferred
    pub total_bytes_to_transfer: u64,
    /// Number of files to copy
    pub files_to_copy: usize,
    /// Number of files to delete
    pub files_to_delete: usize,
    /// Number of conflicts
    pub conflicts: usize,
}

impl DiffResult {
    fn add_action(&mut self, action: SyncAction) {
        match &action {
            SyncAction::CopyToRight { size, .. } | SyncAction::CopyToLeft { size, .. } => {
                self.total_bytes_to_transfer += size;
                self.files_to_copy += 1;
            }
            SyncAction::DeleteRight { .. } | SyncAction::DeleteLeft { .. } => {
                self.files_to_delete += 1;
            }
            SyncAction::Conflict { .. } => {
                self.conflicts += 1;
            }
            SyncAction::CreateDirRight { .. } | SyncAction::CreateDirLeft { .. } => {}
            SyncAction::Skip { .. } => {}
        }
        self.actions.push(action);
    }
}

/// Entry from scan result for easier processing
#[derive(Debug, Clone)]
struct FileEntry {
    size: u64,
    mtime: DateTime<Utc>,
    is_dir: bool,
    hash: Option<String>,
}

/// Compares two scan results with their metadata and produces list of actions.
///
/// # Arguments
/// * `left_scan` - Scan result from left side
/// * `right_scan` - Scan result from right side
/// * `left_meta` - Metadata from left side (previous state)
/// * `right_meta` - Metadata from right side (previous state)
pub fn diff(
    left_scan: &ScanResult,
    right_scan: &ScanResult,
    left_meta: &SyncMetadata,
    right_meta: &SyncMetadata,
) -> DiffResult {
    let mut result = DiffResult::default();

    // Build lookup maps
    let left_files: HashMap<String, FileEntry> = left_scan
        .entries
        .iter()
        .map(|e| {
            (
                e.path.to_string_lossy().to_string(),
                FileEntry {
                    size: e.size,
                    mtime: e.mtime,
                    is_dir: e.is_dir,
                    hash: e.hash.clone(),
                },
            )
        })
        .collect();

    let right_files: HashMap<String, FileEntry> = right_scan
        .entries
        .iter()
        .map(|e| {
            (
                e.path.to_string_lossy().to_string(),
                FileEntry {
                    size: e.size,
                    mtime: e.mtime,
                    is_dir: e.is_dir,
                    hash: e.hash.clone(),
                },
            )
        })
        .collect();

    // Detect case conflicts: paths that differ only in case
    let case_conflicts = detect_case_conflicts(&left_files, &right_files);
    for path in &case_conflicts {
        // Find file info from both sides
        let left_entry = left_files.get(path);
        let right_entry = right_files
            .iter()
            .find(|(p, _)| p.to_lowercase() == path.to_lowercase() && p.as_str() != path)
            .map(|(_, e)| e)
            .or_else(|| right_files.get(path));

        result.add_action(SyncAction::Conflict {
            path: PathBuf::from(path),
            reason: ConflictReason::CaseConflict,
            left: left_entry.map(|e| FileInfo {
                size: e.size,
                mtime: e.mtime,
                hash: e.hash.clone(),
            }),
            right: right_entry.map(|e| FileInfo {
                size: e.size,
                mtime: e.mtime,
                hash: e.hash.clone(),
            }),
        });
    }

    // Process left side entries
    for (path, left_entry) in &left_files {
        // Skip if already handled as case conflict
        if case_conflicts.iter().any(|p| p.to_lowercase() == path.to_lowercase()) {
            continue;
        }
        let right_entry = right_files.get(path);
        let left_prev = left_meta.find_file(path);
        let right_prev = right_meta.find_file(path);
        let right_deleted = right_meta.find_deleted(path);

        let action = determine_action(
            path,
            Some(left_entry),
            right_entry,
            left_prev,
            right_prev,
            right_deleted.is_some(),
            false, // left_deleted
        );

        result.add_action(action);
    }

    // Process right side entries not on left
    for (path, right_entry) in &right_files {
        if left_files.contains_key(path) {
            continue; // Already processed
        }
        // Skip if already handled as case conflict
        if case_conflicts.iter().any(|p| p.to_lowercase() == path.to_lowercase()) {
            continue;
        }

        let left_prev = left_meta.find_file(path);
        let right_prev = right_meta.find_file(path);
        let left_deleted = left_meta.find_deleted(path);

        let action = determine_action(
            path,
            None,
            Some(right_entry),
            left_prev,
            right_prev,
            false, // right_deleted
            left_deleted.is_some(),
        );

        result.add_action(action);
    }

    // Sort actions: directories first, then files
    result.actions.sort_by(|a, b| {
        let a_is_dir = matches!(
            a,
            SyncAction::CreateDirLeft { .. } | SyncAction::CreateDirRight { .. }
        );
        let b_is_dir = matches!(
            b,
            SyncAction::CreateDirLeft { .. } | SyncAction::CreateDirRight { .. }
        );
        b_is_dir.cmp(&a_is_dir)
    });

    result
}

/// Determines what action to take for a specific path
fn determine_action(
    path: &str,
    left: Option<&FileEntry>,
    right: Option<&FileEntry>,
    left_prev: Option<&super::metadata::FileState>,
    right_prev: Option<&super::metadata::FileState>,
    right_deleted: bool,
    left_deleted: bool,
) -> SyncAction {
    let path_buf = PathBuf::from(path);

    match (left, right) {
        // File exists on both sides
        (Some(l), Some(r)) => {
            // Handle directories
            if l.is_dir && r.is_dir {
                return SyncAction::Skip {
                    path: path_buf,
                    reason: "Directory exists on both sides".to_string(),
                };
            }

            // Check if files are the same (within FAT32 tolerance)
            if files_equal(l, r) {
                return SyncAction::Skip {
                    path: path_buf,
                    reason: "Files are identical".to_string(),
                };
            }

            // Files differ - check what changed
            let left_changed = left_prev.is_none() || file_changed_since(l, left_prev.unwrap());
            let right_changed = right_prev.is_none() || file_changed_since(r, right_prev.unwrap());

            match (left_changed, right_changed) {
                (true, true) => SyncAction::Conflict {
                    path: path_buf,
                    reason: ConflictReason::BothModified,
                    left: Some(FileInfo {
                        size: l.size,
                        mtime: l.mtime,
                        hash: l.hash.clone(),
                    }),
                    right: Some(FileInfo {
                        size: r.size,
                        mtime: r.mtime,
                        hash: r.hash.clone(),
                    }),
                },
                (true, false) => SyncAction::CopyToRight {
                    path: path_buf,
                    size: l.size,
                },
                (false, true) => SyncAction::CopyToLeft {
                    path: path_buf,
                    size: r.size,
                },
                (false, false) => SyncAction::Skip {
                    path: path_buf,
                    reason: "No changes detected".to_string(),
                },
            }
        }

        // File only on left side
        (Some(l), None) => {
            if l.is_dir {
                return SyncAction::CreateDirRight { path: path_buf };
            }

            if right_deleted {
                // Was deleted on right - conflict
                SyncAction::Conflict {
                    path: path_buf,
                    reason: ConflictReason::ExistsVsDeleted,
                    left: Some(FileInfo {
                        size: l.size,
                        mtime: l.mtime,
                        hash: l.hash.clone(),
                    }),
                    right: None,
                }
            } else if right_prev.is_some() {
                // Existed before on right but now gone - was deleted
                let left_changed = left_prev.is_none() || file_changed_since(l, left_prev.unwrap());
                if left_changed {
                    // Modified on left, deleted on right - conflict
                    SyncAction::Conflict {
                        path: path_buf,
                        reason: ConflictReason::ModifiedAndDeleted,
                        left: Some(FileInfo {
                            size: l.size,
                            mtime: l.mtime,
                            hash: l.hash.clone(),
                        }),
                        right: None,
                    }
                } else {
                    // Not modified on left, deleted on right - delete left
                    SyncAction::DeleteLeft { path: path_buf }
                }
            } else {
                // New file on left - copy to right
                SyncAction::CopyToRight {
                    path: path_buf,
                    size: l.size,
                }
            }
        }

        // File only on right side
        (None, Some(r)) => {
            if r.is_dir {
                return SyncAction::CreateDirLeft { path: path_buf };
            }

            if left_deleted {
                // Was deleted on left - conflict
                SyncAction::Conflict {
                    path: path_buf,
                    reason: ConflictReason::ExistsVsDeleted,
                    left: None,
                    right: Some(FileInfo {
                        size: r.size,
                        mtime: r.mtime,
                        hash: r.hash.clone(),
                    }),
                }
            } else if left_prev.is_some() {
                // Existed before on left but now gone - was deleted
                let right_changed =
                    right_prev.is_none() || file_changed_since(r, right_prev.unwrap());
                if right_changed {
                    // Modified on right, deleted on left - conflict
                    SyncAction::Conflict {
                        path: path_buf,
                        reason: ConflictReason::ModifiedAndDeleted,
                        left: None,
                        right: Some(FileInfo {
                            size: r.size,
                            mtime: r.mtime,
                            hash: r.hash.clone(),
                        }),
                    }
                } else {
                    // Not modified on right, deleted on left - delete right
                    SyncAction::DeleteRight { path: path_buf }
                }
            } else {
                // New file on right - copy to left
                SyncAction::CopyToLeft {
                    path: path_buf,
                    size: r.size,
                }
            }
        }

        // File on neither side (shouldn't happen)
        (None, None) => SyncAction::Skip {
            path: path_buf,
            reason: "File not found on either side".to_string(),
        },
    }
}

/// Checks if two files are equal (considering FAT32 time tolerance)
fn files_equal(a: &FileEntry, b: &FileEntry) -> bool {
    if a.size != b.size {
        return false;
    }

    // Check mtime with FAT32 tolerance
    let time_diff = (a.mtime - b.mtime).num_seconds().abs();
    if time_diff > FAT32_TOLERANCE_SECS {
        return false;
    }

    // If hashes are available, compare them
    if let (Some(ha), Some(hb)) = (&a.hash, &b.hash) {
        return ha == hb;
    }

    true
}

/// Checks if a file has changed since the recorded state
fn file_changed_since(current: &FileEntry, prev: &super::metadata::FileState) -> bool {
    if current.size != prev.size {
        return true;
    }

    let time_diff = (current.mtime - prev.mtime).num_seconds().abs();
    if time_diff > FAT32_TOLERANCE_SECS {
        return true;
    }

    // If hashes available and differ, file changed
    if let (Some(hc), Some(hp)) = (&current.hash, &prev.hash) {
        if hc != hp {
            return true;
        }
    }

    false
}

/// Detects paths that differ only in case between left and right sides.
/// Returns list of paths (from left side) that have case conflicts.
fn detect_case_conflicts(
    left_files: &HashMap<String, FileEntry>,
    right_files: &HashMap<String, FileEntry>,
) -> Vec<String> {
    use std::collections::HashSet;

    let mut conflicts = HashSet::new();

    // Build case-normalized maps
    let mut left_by_case: HashMap<String, Vec<&str>> = HashMap::new();
    for path in left_files.keys() {
        left_by_case
            .entry(path.to_lowercase())
            .or_default()
            .push(path);
    }

    let mut right_by_case: HashMap<String, Vec<&str>> = HashMap::new();
    for path in right_files.keys() {
        right_by_case
            .entry(path.to_lowercase())
            .or_default()
            .push(path);
    }

    // Check for conflicts within left side (multiple paths with same lowercase)
    for paths in left_by_case.values() {
        if paths.len() > 1 {
            // Multiple files with same case-insensitive name on left
            for path in paths {
                conflicts.insert((*path).to_string());
            }
        }
    }

    // Check for conflicts within right side
    for paths in right_by_case.values() {
        if paths.len() > 1 {
            for path in paths {
                conflicts.insert((*path).to_string());
            }
        }
    }

    // Check for conflicts between sides (same lowercase, different actual case)
    for (normalized, left_paths) in &left_by_case {
        if let Some(right_paths) = right_by_case.get(normalized) {
            // Check if any left path differs from right path
            for lp in left_paths {
                for rp in right_paths {
                    if lp != rp {
                        // Case conflict between sides
                        conflicts.insert((*lp).to_string());
                    }
                }
            }
        }
    }

    conflicts.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::metadata::{FileAttributes, FileState, SyncMetadata};
    use crate::sync::scanner::{FileEntry as ScanFileEntry, ScanResult};
    use chrono::{Duration, Utc};

    fn make_scan_entry(path: &str, size: u64, mtime: DateTime<Utc>) -> ScanFileEntry {
        ScanFileEntry {
            path: PathBuf::from(path),
            size,
            mtime,
            is_dir: false,
            hash: None,
            attributes: FileAttributes::default(),
        }
    }

    fn make_dir_entry(path: &str) -> ScanFileEntry {
        ScanFileEntry {
            path: PathBuf::from(path),
            size: 0,
            mtime: Utc::now(),
            is_dir: true,
            hash: None,
            attributes: FileAttributes::default(),
        }
    }

    fn make_file_state(path: &str, size: u64, mtime: DateTime<Utc>) -> FileState {
        FileState {
            path: path.to_string(),
            size,
            mtime,
            hash: None,
            attributes: FileAttributes::default(),
            last_synced: Utc::now(),
        }
    }

    fn empty_scan(root: &str) -> ScanResult {
        ScanResult {
            root: PathBuf::from(root),
            entries: vec![],
            scan_time: Utc::now(),
            skipped: vec![],
        }
    }

    #[test]
    fn test_new_file_on_left_copies_to_right() {
        let now = Utc::now();

        let mut left_scan = empty_scan("/left");
        left_scan
            .entries
            .push(make_scan_entry("file.txt", 100, now));

        let right_scan = empty_scan("/right");
        let left_meta = SyncMetadata::new();
        let right_meta = SyncMetadata::new();

        let result = diff(&left_scan, &right_scan, &left_meta, &right_meta);

        assert_eq!(result.files_to_copy, 1);
        assert!(matches!(
            &result.actions[0],
            SyncAction::CopyToRight { path, size: 100 } if path == &PathBuf::from("file.txt")
        ));
    }

    #[test]
    fn test_new_file_on_right_copies_to_left() {
        let now = Utc::now();

        let left_scan = empty_scan("/left");
        let mut right_scan = empty_scan("/right");
        right_scan
            .entries
            .push(make_scan_entry("file.txt", 200, now));

        let left_meta = SyncMetadata::new();
        let right_meta = SyncMetadata::new();

        let result = diff(&left_scan, &right_scan, &left_meta, &right_meta);

        assert_eq!(result.files_to_copy, 1);
        assert!(matches!(
            &result.actions[0],
            SyncAction::CopyToLeft { path, size: 200 } if path == &PathBuf::from("file.txt")
        ));
    }

    #[test]
    fn test_same_file_both_sides_skips() {
        let now = Utc::now();

        let mut left_scan = empty_scan("/left");
        left_scan
            .entries
            .push(make_scan_entry("file.txt", 100, now));

        let mut right_scan = empty_scan("/right");
        right_scan
            .entries
            .push(make_scan_entry("file.txt", 100, now));

        let left_meta = SyncMetadata::new();
        let right_meta = SyncMetadata::new();

        let result = diff(&left_scan, &right_scan, &left_meta, &right_meta);

        assert_eq!(result.files_to_copy, 0);
        assert!(matches!(&result.actions[0], SyncAction::Skip { .. }));
    }

    #[test]
    fn test_modified_left_unchanged_right_copies_to_right() {
        let old_time = Utc::now() - Duration::hours(1);
        let new_time = Utc::now();

        let mut left_scan = empty_scan("/left");
        left_scan
            .entries
            .push(make_scan_entry("file.txt", 150, new_time));

        let mut right_scan = empty_scan("/right");
        right_scan
            .entries
            .push(make_scan_entry("file.txt", 100, old_time));

        let mut left_meta = SyncMetadata::new();
        left_meta
            .files
            .push(make_file_state("file.txt", 100, old_time));

        let mut right_meta = SyncMetadata::new();
        right_meta
            .files
            .push(make_file_state("file.txt", 100, old_time));

        let result = diff(&left_scan, &right_scan, &left_meta, &right_meta);

        assert_eq!(result.files_to_copy, 1);
        assert!(matches!(
            &result.actions[0],
            SyncAction::CopyToRight { path, .. } if path == &PathBuf::from("file.txt")
        ));
    }

    #[test]
    fn test_both_modified_creates_conflict() {
        let old_time = Utc::now() - Duration::hours(1);
        let new_time = Utc::now();

        let mut left_scan = empty_scan("/left");
        left_scan
            .entries
            .push(make_scan_entry("file.txt", 150, new_time));

        let mut right_scan = empty_scan("/right");
        right_scan
            .entries
            .push(make_scan_entry("file.txt", 200, new_time));

        let mut left_meta = SyncMetadata::new();
        left_meta
            .files
            .push(make_file_state("file.txt", 100, old_time));

        let mut right_meta = SyncMetadata::new();
        right_meta
            .files
            .push(make_file_state("file.txt", 100, old_time));

        let result = diff(&left_scan, &right_scan, &left_meta, &right_meta);

        assert_eq!(result.conflicts, 1);
        assert!(matches!(
            &result.actions[0],
            SyncAction::Conflict {
                reason: ConflictReason::BothModified,
                ..
            }
        ));
    }

    #[test]
    fn test_deleted_left_unchanged_right_deletes_right() {
        let old_time = Utc::now() - Duration::hours(1);

        let left_scan = empty_scan("/left");

        let mut right_scan = empty_scan("/right");
        right_scan
            .entries
            .push(make_scan_entry("file.txt", 100, old_time));

        let mut left_meta = SyncMetadata::new();
        left_meta
            .files
            .push(make_file_state("file.txt", 100, old_time));

        let mut right_meta = SyncMetadata::new();
        right_meta
            .files
            .push(make_file_state("file.txt", 100, old_time));

        let result = diff(&left_scan, &right_scan, &left_meta, &right_meta);

        assert_eq!(result.files_to_delete, 1);
        assert!(matches!(
            &result.actions[0],
            SyncAction::DeleteRight { path } if path == &PathBuf::from("file.txt")
        ));
    }

    #[test]
    fn test_deleted_left_modified_right_creates_conflict() {
        let old_time = Utc::now() - Duration::hours(1);
        let new_time = Utc::now();

        let left_scan = empty_scan("/left");

        let mut right_scan = empty_scan("/right");
        right_scan
            .entries
            .push(make_scan_entry("file.txt", 200, new_time));

        let mut left_meta = SyncMetadata::new();
        left_meta
            .files
            .push(make_file_state("file.txt", 100, old_time));

        let mut right_meta = SyncMetadata::new();
        right_meta
            .files
            .push(make_file_state("file.txt", 100, old_time));

        let result = diff(&left_scan, &right_scan, &left_meta, &right_meta);

        assert_eq!(result.conflicts, 1);
        assert!(matches!(
            &result.actions[0],
            SyncAction::Conflict {
                reason: ConflictReason::ModifiedAndDeleted,
                ..
            }
        ));
    }

    #[test]
    fn test_fat32_time_tolerance() {
        let time1 = Utc::now();
        let time2 = time1 + Duration::seconds(1); // Within 2 sec tolerance

        let mut left_scan = empty_scan("/left");
        left_scan
            .entries
            .push(make_scan_entry("file.txt", 100, time1));

        let mut right_scan = empty_scan("/right");
        right_scan
            .entries
            .push(make_scan_entry("file.txt", 100, time2));

        let left_meta = SyncMetadata::new();
        let right_meta = SyncMetadata::new();

        let result = diff(&left_scan, &right_scan, &left_meta, &right_meta);

        // Should be considered equal due to FAT32 tolerance
        assert!(matches!(&result.actions[0], SyncAction::Skip { .. }));
    }

    #[test]
    fn test_first_sync_no_metadata() {
        let now = Utc::now();

        let mut left_scan = empty_scan("/left");
        left_scan
            .entries
            .push(make_scan_entry("left_only.txt", 100, now));
        left_scan
            .entries
            .push(make_scan_entry("common.txt", 50, now));

        let mut right_scan = empty_scan("/right");
        right_scan
            .entries
            .push(make_scan_entry("right_only.txt", 200, now));
        right_scan
            .entries
            .push(make_scan_entry("common.txt", 50, now));

        let left_meta = SyncMetadata::new();
        let right_meta = SyncMetadata::new();

        let result = diff(&left_scan, &right_scan, &left_meta, &right_meta);

        // left_only.txt should copy to right
        // right_only.txt should copy to left
        // common.txt should be skipped (same)
        assert_eq!(result.files_to_copy, 2);
        assert_eq!(result.total_bytes_to_transfer, 300); // 100 + 200
    }

    #[test]
    fn test_new_directory_on_left() {
        let mut left_scan = empty_scan("/left");
        left_scan.entries.push(make_dir_entry("subdir"));

        let right_scan = empty_scan("/right");
        let left_meta = SyncMetadata::new();
        let right_meta = SyncMetadata::new();

        let result = diff(&left_scan, &right_scan, &left_meta, &right_meta);

        assert!(matches!(
            &result.actions[0],
            SyncAction::CreateDirRight { path } if path == &PathBuf::from("subdir")
        ));
    }

    #[test]
    fn test_directory_on_both_sides_skips() {
        let mut left_scan = empty_scan("/left");
        left_scan.entries.push(make_dir_entry("subdir"));

        let mut right_scan = empty_scan("/right");
        right_scan.entries.push(make_dir_entry("subdir"));

        let left_meta = SyncMetadata::new();
        let right_meta = SyncMetadata::new();

        let result = diff(&left_scan, &right_scan, &left_meta, &right_meta);

        assert!(matches!(&result.actions[0], SyncAction::Skip { .. }));
    }
}
