//! Application state types and enums

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::project::ProjectSettings;
use crate::sync::differ::{DiffResult, SyncAction};
use crate::sync::executor::{
    CompletedAction, ExecutionResult, FailedAction, FileSnapshot, SkippedAction, SyncErrorKind,
};
use crate::sync::scanner::ScanResult;

/// Application screens
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    ProjectList,
    ProjectView,
    Analyzing,
    Preview,
    Syncing,
    SyncComplete,
}

/// Dialog mode for project list screen
#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    None,
    NewProject(NewProjectDialog),
    DeleteConfirm(String),
    CreateDirConfirm { path: PathBuf, is_left: bool },
    Error(String),
    SyncConfirm(SyncConfirmDialog),
    CancelSyncConfirm,
    ExclusionsInfo(ExclusionsInfoDialog),
    DiskSpaceWarning(DiskSpaceWarningDialog),
    FileError(FileErrorDialog),
    ProjectSettings(SettingsDialog),
}

/// Disk space warning dialog
#[derive(Debug, Clone, PartialEq)]
pub struct DiskSpaceWarningDialog {
    /// Which side has insufficient space (true = left, false = right)
    pub is_left: bool,
    /// Path being checked
    pub path: PathBuf,
    /// Available space in bytes
    pub available: u64,
    /// Required space in bytes
    pub required: u64,
}

/// File error dialog (locked file, permission denied)
#[derive(Debug, Clone, PartialEq)]
pub struct FileErrorDialog {
    /// Path to the file that failed
    pub path: PathBuf,
    /// Error message
    pub error: String,
    /// Error classification
    pub kind: SyncErrorKind,
    /// The action that failed (for retry)
    pub action: SyncAction,
}

/// Filter mode for preview
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewFilter {
    #[default]
    All,
    Changes,
    Conflicts,
}

impl PreviewFilter {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Changes,
            Self::Changes => Self::Conflicts,
            Self::Conflicts => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Changes => "Changes",
            Self::Conflicts => "Conflicts",
        }
    }
}

/// Dialog input fields
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogField {
    Name,
    LeftPath,
    RightPath,
}

/// New project dialog state
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProjectDialog {
    pub name: String,
    pub left_path: String,
    pub right_path: String,
    pub focused_field: DialogField,
    pub error: Option<String>,
}

impl NewProjectDialog {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            left_path: String::new(),
            right_path: String::new(),
            focused_field: DialogField::Name,
            error: None,
        }
    }

    pub fn focused_value_mut(&mut self) -> &mut String {
        match self.focused_field {
            DialogField::Name => &mut self.name,
            DialogField::LeftPath => &mut self.left_path,
            DialogField::RightPath => &mut self.right_path,
        }
    }

    pub fn next_field(&mut self) {
        self.focused_field = match self.focused_field {
            DialogField::Name => DialogField::LeftPath,
            DialogField::LeftPath => DialogField::RightPath,
            DialogField::RightPath => DialogField::Name,
        };
    }

    pub fn prev_field(&mut self) {
        self.focused_field = match self.focused_field {
            DialogField::Name => DialogField::RightPath,
            DialogField::LeftPath => DialogField::Name,
            DialogField::RightPath => DialogField::LeftPath,
        };
    }
}

impl Default for NewProjectDialog {
    fn default() -> Self {
        Self::new()
    }
}

/// Sync confirmation dialog data
#[derive(Debug, Clone, PartialEq)]
pub struct SyncConfirmDialog {
    pub files_to_copy: usize,
    pub files_to_delete: usize,
    pub bytes_to_transfer: u64,
    pub dirs_to_create: usize,
}

/// Exclusions info dialog data
#[derive(Debug, Clone, PartialEq)]
pub struct ExclusionsInfoDialog {
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    pub left_exists: bool,
    pub right_exists: bool,
    pub left_count: usize,
    pub right_count: usize,
}

/// Settings dialog field selector
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    BackupVersions,
    DeletedRetentionDays,
    SoftDelete,
    VerifyHash,
}

/// Project settings dialog state
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsDialog {
    pub backup_versions: String,
    pub deleted_retention_days: String,
    pub soft_delete: bool,
    pub verify_hash: bool,
    pub focused_field: SettingsField,
    pub error: Option<String>,
}

impl SettingsDialog {
    pub fn from_settings(settings: &ProjectSettings) -> Self {
        Self {
            backup_versions: settings.backup_versions.to_string(),
            deleted_retention_days: settings.deleted_retention_days.to_string(),
            soft_delete: settings.soft_delete,
            verify_hash: settings.verify_hash,
            focused_field: SettingsField::BackupVersions,
            error: None,
        }
    }

    pub fn to_settings(&self) -> Result<ProjectSettings, String> {
        let backup_versions = self
            .backup_versions
            .parse::<usize>()
            .map_err(|_| "Invalid backup versions")?;
        if backup_versions == 0 || backup_versions > 100 {
            return Err("Backup versions must be 1-100".to_string());
        }

        let deleted_retention_days = self
            .deleted_retention_days
            .parse::<u32>()
            .map_err(|_| "Invalid retention days")?;
        if deleted_retention_days > 365 {
            return Err("Retention days must be 0-365 (0=off)".to_string());
        }

        Ok(ProjectSettings {
            backup_versions,
            deleted_retention_days,
            soft_delete: self.soft_delete,
            verify_hash: self.verify_hash,
        })
    }

    pub fn focused_value_mut(&mut self) -> Option<&mut String> {
        match self.focused_field {
            SettingsField::BackupVersions => Some(&mut self.backup_versions),
            SettingsField::DeletedRetentionDays => Some(&mut self.deleted_retention_days),
            SettingsField::SoftDelete | SettingsField::VerifyHash => None,
        }
    }

    pub fn toggle_focused_bool(&mut self) {
        match self.focused_field {
            SettingsField::SoftDelete => self.soft_delete = !self.soft_delete,
            SettingsField::VerifyHash => self.verify_hash = !self.verify_hash,
            _ => {}
        }
    }

    pub fn next_field(&mut self) {
        self.focused_field = match self.focused_field {
            SettingsField::BackupVersions => SettingsField::DeletedRetentionDays,
            SettingsField::DeletedRetentionDays => SettingsField::SoftDelete,
            SettingsField::SoftDelete => SettingsField::VerifyHash,
            SettingsField::VerifyHash => SettingsField::BackupVersions,
        };
    }

    pub fn prev_field(&mut self) {
        self.focused_field = match self.focused_field {
            SettingsField::BackupVersions => SettingsField::VerifyHash,
            SettingsField::DeletedRetentionDays => SettingsField::BackupVersions,
            SettingsField::SoftDelete => SettingsField::DeletedRetentionDays,
            SettingsField::VerifyHash => SettingsField::SoftDelete,
        };
    }
}

/// Action that user can modify
#[derive(Debug, Clone, PartialEq)]
pub enum UserAction {
    /// Keep the original action from diff
    Original(SyncAction),
    /// User changed to copy left to right
    CopyToRight { path: PathBuf, size: u64 },
    /// User changed to copy right to left
    CopyToLeft { path: PathBuf, size: u64 },
    /// User changed to delete from left
    DeleteLeft { path: PathBuf },
    /// User changed to delete from right
    DeleteRight { path: PathBuf },
    /// User chose to skip this item
    Skip { path: PathBuf },
}

impl UserAction {
    pub fn path(&self) -> &PathBuf {
        match self {
            Self::Original(action) => action.path(),
            Self::CopyToRight { path, .. } => path,
            Self::CopyToLeft { path, .. } => path,
            Self::DeleteLeft { path } => path,
            Self::DeleteRight { path } => path,
            Self::Skip { path } => path,
        }
    }

    pub fn is_modified(&self) -> bool {
        !matches!(self, Self::Original(_))
    }

    /// Converts UserAction to SyncAction for execution.
    /// Returns None for Skip and Conflict actions.
    pub fn to_sync_action(&self) -> Option<SyncAction> {
        match self {
            UserAction::Original(action) => match action {
                SyncAction::Skip { .. } | SyncAction::Conflict { .. } => None,
                _ => Some(action.clone()),
            },
            UserAction::CopyToRight { path, size } => Some(SyncAction::CopyToRight {
                path: path.clone(),
                size: *size,
            }),
            UserAction::CopyToLeft { path, size } => Some(SyncAction::CopyToLeft {
                path: path.clone(),
                size: *size,
            }),
            UserAction::DeleteLeft { path } => Some(SyncAction::DeleteLeft {
                path: path.clone(),
            }),
            UserAction::DeleteRight { path } => Some(SyncAction::DeleteRight {
                path: path.clone(),
            }),
            UserAction::Skip { .. } => None,
        }
    }
}

/// Preview summary statistics
#[derive(Debug, Default)]
pub struct PreviewSummary {
    pub copy_to_right: usize,
    pub copy_to_left: usize,
    pub bytes_to_right: u64,
    pub bytes_to_left: u64,
    pub delete_right: usize,
    pub delete_left: usize,
    pub conflicts: usize,
    pub dirs_to_create: usize,
    pub skipped: usize,
}

/// Preview state
#[derive(Debug, Default)]
pub struct PreviewState {
    pub actions: Vec<UserAction>,
    pub filter: PreviewFilter,
    pub selected: usize,
    pub scroll_offset: usize,
    pub selected_items: HashSet<usize>,
    pub left_scan: Option<ScanResult>,
    pub right_scan: Option<ScanResult>,
}

impl PreviewState {
    pub fn new(diff_result: DiffResult, left_scan: ScanResult, right_scan: ScanResult) -> Self {
        Self {
            actions: diff_result
                .actions
                .into_iter()
                .map(UserAction::Original)
                .collect(),
            filter: PreviewFilter::All,
            selected: 0,
            scroll_offset: 0,
            selected_items: HashSet::new(),
            left_scan: Some(left_scan),
            right_scan: Some(right_scan),
        }
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        self.actions
            .iter()
            .enumerate()
            .filter(|(_, action)| match self.filter {
                PreviewFilter::All => true,
                PreviewFilter::Changes => !is_skip_action(action),
                PreviewFilter::Conflicts => is_conflict_action(action),
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn summary(&self) -> PreviewSummary {
        let mut summary = PreviewSummary::default();
        for action in &self.actions {
            match action {
                UserAction::Original(SyncAction::CopyToRight { size, .. })
                | UserAction::CopyToRight { size, .. } => {
                    summary.copy_to_right += 1;
                    summary.bytes_to_right += size;
                }
                UserAction::Original(SyncAction::CopyToLeft { size, .. })
                | UserAction::CopyToLeft { size, .. } => {
                    summary.copy_to_left += 1;
                    summary.bytes_to_left += size;
                }
                UserAction::Original(SyncAction::DeleteRight { .. })
                | UserAction::DeleteRight { .. } => {
                    summary.delete_right += 1;
                }
                UserAction::Original(SyncAction::DeleteLeft { .. })
                | UserAction::DeleteLeft { .. } => {
                    summary.delete_left += 1;
                }
                UserAction::Original(SyncAction::Conflict { .. }) => {
                    summary.conflicts += 1;
                }
                UserAction::Original(SyncAction::CreateDirRight { .. }) => {
                    summary.dirs_to_create += 1;
                }
                UserAction::Original(SyncAction::CreateDirLeft { .. }) => {
                    summary.dirs_to_create += 1;
                }
                UserAction::Skip { .. } | UserAction::Original(SyncAction::Skip { .. }) => {
                    summary.skipped += 1;
                }
            }
        }
        summary
    }

    /// Get file size from left scan by path
    pub fn get_file_size_from_left(&self, path: &Path) -> Option<u64> {
        self.left_scan
            .as_ref()?
            .entries
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.size)
    }

    /// Get file size from right scan by path
    pub fn get_file_size_from_right(&self, path: &Path) -> Option<u64> {
        self.right_scan
            .as_ref()?
            .entries
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.size)
    }
}

/// State during sync execution
#[derive(Debug)]
pub struct SyncingState {
    pub total_actions: usize,
    pub completed_actions: usize,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub current_file: PathBuf,
    pub start_time: Instant,
    pub cancel_requested: bool,
    pub current_index: usize,
    pub actions: Vec<SyncAction>,
    pub snapshots: HashMap<PathBuf, FileSnapshot>,
    pub result: ExecutionResult,
}

impl SyncingState {
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn estimated_remaining(&self) -> Option<Duration> {
        if self.completed_actions == 0 {
            return None;
        }
        let elapsed = self.elapsed();
        let rate = self.completed_actions as f64 / elapsed.as_secs_f64();
        if rate <= 0.0 {
            return None;
        }
        let remaining = self.total_actions - self.completed_actions;
        Some(Duration::from_secs_f64(remaining as f64 / rate))
    }
}

/// State after sync completion
#[derive(Debug)]
pub struct SyncCompleteState {
    pub completed: Vec<CompletedAction>,
    pub failed: Vec<FailedAction>,
    pub skipped: Vec<SkippedAction>,
    pub duration: Duration,
    pub bytes_transferred: u64,
    pub scroll_offset: usize,
    pub changed_during_sync: Vec<PathBuf>,
}

// Helper functions for action filtering

pub fn is_skip_action(action: &UserAction) -> bool {
    matches!(
        action,
        UserAction::Skip { .. } | UserAction::Original(SyncAction::Skip { .. })
    )
}

pub fn is_conflict_action(action: &UserAction) -> bool {
    matches!(action, UserAction::Original(SyncAction::Conflict { .. }))
}
