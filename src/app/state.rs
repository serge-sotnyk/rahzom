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
    CreateDirConfirm {
        path: PathBuf,
        is_left: bool,
    },
    Error(String),
    SyncConfirm(SyncConfirmDialog),
    CancelSyncConfirm,
    ExclusionsInfo(ExclusionsInfoDialog),
    DiskSpaceWarning(DiskSpaceWarningDialog),
    FileError(FileErrorDialog),
    ProjectSettings(SettingsDialog),
    /// Details popup for a single preview action
    ActionDetails {
        action_index: usize,
    },
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
    All,
    #[default]
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

/// Sort key for preview list
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewSort {
    #[default]
    Path,
    Type,
    Size,
}

impl PreviewSort {
    pub fn next(self) -> Self {
        match self {
            Self::Path => Self::Type,
            Self::Type => Self::Size,
            Self::Size => Self::Path,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Path => "Path",
            Self::Type => "Type",
            Self::Size => "Size",
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
    DeleteLeft {
        path: PathBuf,
        size: u64,
        is_directory: bool,
    },
    /// User changed to delete from right
    DeleteRight {
        path: PathBuf,
        size: u64,
        is_directory: bool,
    },
    /// User chose to skip this item
    Skip {
        path: PathBuf,
        size: u64,
        is_directory: bool,
    },
}

impl UserAction {
    pub fn path(&self) -> &PathBuf {
        match self {
            Self::Original(action) => action.path(),
            Self::CopyToRight { path, .. } => path,
            Self::CopyToLeft { path, .. } => path,
            Self::DeleteLeft { path, .. } => path,
            Self::DeleteRight { path, .. } => path,
            Self::Skip { path, .. } => path,
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
            UserAction::DeleteLeft {
                path,
                size,
                is_directory,
            } => Some(SyncAction::DeleteLeft {
                path: path.clone(),
                size: *size,
                is_directory: *is_directory,
            }),
            UserAction::DeleteRight {
                path,
                size,
                is_directory,
            } => Some(SyncAction::DeleteRight {
                path: path.clone(),
                size: *size,
                is_directory: *is_directory,
            }),
            UserAction::Skip { .. } => None,
        }
    }

    /// Effective `size` value of this action's underlying file (0 for non-file
    /// targets and dir entries — directory aggregates are computed separately).
    pub fn payload_size(&self) -> u64 {
        match self {
            UserAction::Original(SyncAction::CopyToRight { size, .. })
            | UserAction::Original(SyncAction::CopyToLeft { size, .. })
            | UserAction::Original(SyncAction::DeleteRight { size, .. })
            | UserAction::Original(SyncAction::DeleteLeft { size, .. })
            | UserAction::Original(SyncAction::Skip { size, .. })
            | UserAction::CopyToRight { size, .. }
            | UserAction::CopyToLeft { size, .. }
            | UserAction::DeleteRight { size, .. }
            | UserAction::DeleteLeft { size, .. }
            | UserAction::Skip { size, .. } => *size,
            UserAction::Original(SyncAction::Conflict { left, right, .. }) => {
                let ls = left.as_ref().map(|f| f.size).unwrap_or(0);
                let rs = right.as_ref().map(|f| f.size).unwrap_or(0);
                ls.max(rs)
            }
            UserAction::Original(SyncAction::CreateDirRight { .. })
            | UserAction::Original(SyncAction::CreateDirLeft { .. }) => 0,
        }
    }

    /// Whether this action targets a directory.
    pub fn is_directory(&self) -> bool {
        match self {
            UserAction::Original(SyncAction::CreateDirLeft { .. })
            | UserAction::Original(SyncAction::CreateDirRight { .. }) => true,
            UserAction::Original(SyncAction::Skip { is_directory, .. })
            | UserAction::Original(SyncAction::DeleteLeft { is_directory, .. })
            | UserAction::Original(SyncAction::DeleteRight { is_directory, .. })
            | UserAction::Skip { is_directory, .. }
            | UserAction::DeleteLeft { is_directory, .. }
            | UserAction::DeleteRight { is_directory, .. } => *is_directory,
            UserAction::Original(SyncAction::Conflict { left, right, .. }) => {
                let l = left.as_ref().map(|f| f.is_directory).unwrap_or(false);
                let r = right.as_ref().map(|f| f.is_directory).unwrap_or(false);
                l || r
            }
            UserAction::Original(SyncAction::CopyToRight { .. })
            | UserAction::Original(SyncAction::CopyToLeft { .. })
            | UserAction::CopyToRight { .. }
            | UserAction::CopyToLeft { .. } => false,
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
    pub sort: PreviewSort,
    pub selected: usize,
    pub scroll_offset: usize,
    pub selected_items: HashSet<usize>,
    pub left_scan: Option<ScanResult>,
    pub right_scan: Option<ScanResult>,
    /// Aggregated subtree size for every action whose path targets a directory.
    /// Rebuilt by `recompute_view` on construction and after every mutation.
    pub(crate) dir_sizes: HashMap<PathBuf, u64>,
}

impl PreviewState {
    pub fn new(diff_result: DiffResult, left_scan: ScanResult, right_scan: ScanResult) -> Self {
        let mut state = Self {
            actions: diff_result
                .actions
                .into_iter()
                .map(UserAction::Original)
                .collect(),
            filter: PreviewFilter::default(),
            sort: PreviewSort::default(),
            selected: 0,
            scroll_offset: 0,
            selected_items: HashSet::new(),
            left_scan: Some(left_scan),
            right_scan: Some(right_scan),
            dir_sizes: HashMap::new(),
        };
        state.recompute_view();
        state
    }

    /// Replace one action and refresh derived view state.
    /// All mutation of `actions` from outside this module must go through here.
    pub fn replace_action(&mut self, real_idx: usize, new_action: UserAction) {
        if real_idx < self.actions.len() {
            self.actions[real_idx] = new_action;
            self.recompute_view();
        }
    }

    /// Rebuild derived state (currently `dir_sizes`). The action list itself
    /// is *not* reordered — view order is produced on demand by
    /// `sorted_filtered_indices`, so existing real indices in `selected_items`
    /// stay valid across sort and filter changes.
    fn recompute_view(&mut self) {
        let dir_paths: HashSet<PathBuf> = self
            .actions
            .iter()
            .filter(|a| a.is_directory())
            .map(|a| a.path().clone())
            .collect();

        let mut sizes: HashMap<PathBuf, u64> =
            dir_paths.iter().map(|p| (p.clone(), 0u64)).collect();

        for action in &self.actions {
            if action.is_directory() {
                continue;
            }
            let size = action.payload_size();
            if size == 0 {
                continue;
            }
            for ancestor in action.path().ancestors().skip(1) {
                if ancestor.as_os_str().is_empty() {
                    continue;
                }
                if let Some(slot) = sizes.get_mut(ancestor) {
                    *slot += size;
                }
            }
        }

        self.dir_sizes = sizes;
    }

    /// Effective size used by Size sort and the UI tag.
    /// - File actions: the file's own size (`payload_size`).
    /// - Directory actions: the cached aggregated subtree size.
    /// - Conflict: `max(file_side_size, aggregated_subtree_size)`. This
    ///   covers file-vs-file (subtree=0), dir-vs-dir (file side=0), and the
    ///   file-vs-directory mismatch (both contribute) without losing weight.
    pub fn effective_size(&self, action: &UserAction) -> u64 {
        if let UserAction::Original(SyncAction::Conflict { left, right, .. }) = action {
            let file_side = left
                .as_ref()
                .filter(|f| !f.is_directory)
                .map(|f| f.size)
                .unwrap_or(0)
                .max(
                    right
                        .as_ref()
                        .filter(|f| !f.is_directory)
                        .map(|f| f.size)
                        .unwrap_or(0),
                );
            let dir_aggregate = self.dir_sizes.get(action.path()).copied().unwrap_or(0);
            return file_side.max(dir_aggregate);
        }
        if action.is_directory() {
            self.dir_sizes.get(action.path()).copied().unwrap_or(0)
        } else {
            action.payload_size()
        }
    }

    /// Indices into `actions` that pass the current filter, in original order.
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

    /// Indices into `actions` that pass the current filter, sorted by the
    /// current sort key. This is the rendered list. Note: `actions` itself is
    /// never reordered, so `selected_items` (which holds real indices) stays
    /// valid across any sort / filter change.
    pub fn sorted_filtered_indices(&self) -> Vec<usize> {
        let mut indices = self.filtered_indices();
        indices.sort_by(|&a, &b| {
            let aa = &self.actions[a];
            let bb = &self.actions[b];
            self.sort_key_cmp(aa, bb)
        });
        indices
    }

    fn sort_key_cmp(&self, a: &UserAction, b: &UserAction) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let path_cmp = || hierarchical_path_key(a).cmp(&hierarchical_path_key(b));
        match self.sort {
            PreviewSort::Path => path_cmp(),
            PreviewSort::Type => {
                // Modified entries come first, each section sorted by type then path.
                let am = !a.is_modified();
                let bm = !b.is_modified();
                am.cmp(&bm)
                    .then_with(|| type_rank(a).cmp(&type_rank(b)))
                    .then_with(path_cmp)
            }
            PreviewSort::Size => {
                // Largest first. Tie-break by hierarchical path.
                match self.effective_size(b).cmp(&self.effective_size(a)) {
                    Ordering::Equal => path_cmp(),
                    ord => ord,
                }
            }
        }
    }

    /// Path of the currently selected entry in the rendered list, if any.
    pub fn capture_selected_path(&self) -> Option<PathBuf> {
        let indices = self.sorted_filtered_indices();
        let real = *indices.get(self.selected)?;
        Some(self.actions[real].path().clone())
    }

    /// Move `selected` to the entry whose path matches `path` in the new
    /// rendered list, or to the largest index that is strictly smaller, or
    /// to 0 if that doesn't exist.
    pub fn restore_selection_to_path(&mut self, path: Option<PathBuf>) {
        let indices = self.sorted_filtered_indices();
        if indices.is_empty() {
            self.selected = 0;
            self.scroll_offset = 0;
            return;
        }
        if let Some(target) = path {
            if let Some(pos) = indices
                .iter()
                .position(|&i| self.actions[i].path() == &target)
            {
                self.selected = pos;
                return;
            }
        }
        if self.selected >= indices.len() {
            self.selected = indices.len() - 1;
        }
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

/// Type rank for sorting. Lower = earlier.
fn type_rank(action: &UserAction) -> u8 {
    match action {
        UserAction::Original(SyncAction::Conflict { .. }) => 0,
        UserAction::Original(SyncAction::CopyToRight { .. })
        | UserAction::CopyToRight { .. }
        | UserAction::Original(SyncAction::CreateDirRight { .. }) => 1,
        UserAction::Original(SyncAction::CopyToLeft { .. })
        | UserAction::CopyToLeft { .. }
        | UserAction::Original(SyncAction::CreateDirLeft { .. }) => 2,
        UserAction::Original(SyncAction::DeleteRight { .. })
        | UserAction::DeleteRight { .. }
        | UserAction::Original(SyncAction::DeleteLeft { .. })
        | UserAction::DeleteLeft { .. } => 3,
        UserAction::Original(SyncAction::Skip { .. }) | UserAction::Skip { .. } => 4,
    }
}

/// Hierarchical path sort key. Each path component contributes a triple
/// `(kind, name_lower, leaf_marker)`:
/// - kind = 0 if this position is a file (only at the leaf), 1 if it's a
///   directory (leaf or intermediate);
/// - name_lower = lower-cased component name;
/// - leaf_marker = 0 if this is the last component of the path, 1 if more
///   components follow.
///
/// Lexicographic comparison of the resulting `Vec` produces a tree-order:
/// at every directory level, files come before subfolders, and a folder
/// entry is immediately followed by its own contents recursively before the
/// next sibling opens.
fn hierarchical_path_key(action: &UserAction) -> Vec<(u8, String, u8)> {
    let path = action.path();
    let is_dir = action.is_directory();
    let components: Vec<String> = path
        .components()
        .map(|c| match c {
            std::path::Component::Normal(s) => s.to_string_lossy().to_lowercase(),
            // Treat any non-normal component (root, prefix, ".", "..") as a
            // bare lower-cased segment so we still produce a stable key.
            other => other.as_os_str().to_string_lossy().to_lowercase(),
        })
        .collect();

    if components.is_empty() {
        return Vec::new();
    }

    let last = components.len() - 1;
    components
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let leaf_marker: u8 = if i == last { 0 } else { 1 };
            let kind: u8 = if i == last && !is_dir { 0 } else { 1 };
            (kind, name, leaf_marker)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::differ::{ConflictReason, SyncAction};

    fn mk_state(actions: Vec<UserAction>) -> PreviewState {
        let mut state = PreviewState {
            actions,
            ..Default::default()
        };
        // Mirror what `PreviewState::new` does, so tests see the same derived
        // state (`dir_sizes`) the runtime would.
        state.recompute_view();
        state
    }

    fn copy_right(p: &str, size: u64) -> UserAction {
        UserAction::Original(SyncAction::CopyToRight {
            path: PathBuf::from(p),
            size,
        })
    }

    fn copy_left(p: &str, size: u64) -> UserAction {
        UserAction::Original(SyncAction::CopyToLeft {
            path: PathBuf::from(p),
            size,
        })
    }

    fn delete_right(p: &str) -> UserAction {
        UserAction::Original(SyncAction::DeleteRight {
            path: PathBuf::from(p),
            size: 0,
            is_directory: false,
        })
    }

    fn skip(p: &str) -> UserAction {
        UserAction::Original(SyncAction::Skip {
            path: PathBuf::from(p),
            reason: "excluded".to_string(),
            size: 0,
            is_directory: false,
        })
    }

    fn skip_file(p: &str, size: u64) -> UserAction {
        UserAction::Original(SyncAction::Skip {
            path: PathBuf::from(p),
            reason: "identical".to_string(),
            size,
            is_directory: false,
        })
    }

    fn skip_dir(p: &str) -> UserAction {
        UserAction::Original(SyncAction::Skip {
            path: PathBuf::from(p),
            reason: "directory".to_string(),
            size: 0,
            is_directory: true,
        })
    }

    fn conflict(p: &str) -> UserAction {
        UserAction::Original(SyncAction::Conflict {
            path: PathBuf::from(p),
            reason: ConflictReason::BothModified,
            left: None,
            right: None,
        })
    }

    fn conflict_dir(p: &str) -> UserAction {
        // Directory present on both sides, both modified — synthesize a
        // conflict that our `is_directory` helper recognises as a dir.
        UserAction::Original(SyncAction::Conflict {
            path: PathBuf::from(p),
            reason: ConflictReason::BothModified,
            left: Some(crate::sync::differ::FileInfo {
                size: 0,
                mtime: chrono::Utc::now(),
                hash: None,
                is_directory: true,
            }),
            right: Some(crate::sync::differ::FileInfo {
                size: 0,
                mtime: chrono::Utc::now(),
                hash: None,
                is_directory: true,
            }),
        })
    }

    fn paths_in_order(state: &PreviewState) -> Vec<String> {
        state
            .sorted_filtered_indices()
            .into_iter()
            .map(|i| state.actions[i].path().display().to_string())
            .collect()
    }

    #[test]
    fn sort_by_path_is_case_insensitive() {
        let mut state = mk_state(vec![
            copy_right("Banana.txt", 1),
            copy_right("apple.txt", 1),
            copy_right("Cherry.txt", 1),
        ]);
        state.filter = PreviewFilter::All;
        state.sort = PreviewSort::Path;
        assert_eq!(
            paths_in_order(&state),
            vec!["apple.txt", "Banana.txt", "Cherry.txt"]
        );
    }

    #[test]
    fn sort_by_type_modified_first_then_groups() {
        let mut state = mk_state(vec![
            skip("z_skip.txt"),
            copy_right("a_copy_right.txt", 5),
            copy_left("b_copy_left.txt", 5),
            delete_right("c_delete.txt"),
            conflict("d_conflict.txt"),
            // User-modified copy of an originally-skipped file.
            UserAction::CopyToRight {
                path: PathBuf::from("user_modified.txt"),
                size: 1,
            },
        ]);
        state.filter = PreviewFilter::All;
        state.sort = PreviewSort::Type;
        let order = paths_in_order(&state);

        // Modified entries come first.
        assert_eq!(order[0], "user_modified.txt");
        // Then non-modified, by type rank: Conflict < Copy→ < Copy← < Delete < Skip.
        assert_eq!(
            order[1..],
            [
                "d_conflict.txt",
                "a_copy_right.txt",
                "b_copy_left.txt",
                "c_delete.txt",
                "z_skip.txt",
            ]
        );
    }

    #[test]
    fn sort_by_size_descending_files_first() {
        let mut state = mk_state(vec![
            copy_right("small.txt", 100),
            copy_right("huge.txt", 100_000),
            skip("zero.txt"),
            copy_right("medium.txt", 5_000),
        ]);
        state.filter = PreviewFilter::All;
        state.sort = PreviewSort::Size;
        assert_eq!(
            paths_in_order(&state),
            vec!["huge.txt", "medium.txt", "small.txt", "zero.txt"]
        );
    }

    #[test]
    fn changes_filter_excludes_skips() {
        let mut state = mk_state(vec![
            copy_right("a.txt", 1),
            skip("b.txt"),
            conflict("c.txt"),
        ]);
        state.filter = PreviewFilter::Changes;
        state.sort = PreviewSort::Path;
        assert_eq!(paths_in_order(&state), vec!["a.txt", "c.txt"]);
    }

    #[test]
    fn restore_selection_keeps_path_through_filter_change() {
        let mut state = mk_state(vec![
            copy_right("a.txt", 1),
            skip("b.txt"),
            copy_right("c.txt", 1),
        ]);
        state.filter = PreviewFilter::All;
        state.sort = PreviewSort::Path;
        // Select "b.txt".
        state.selected = 1;
        let pinned = state.capture_selected_path();
        assert_eq!(pinned.as_deref(), Some(Path::new("b.txt")));

        // Switch to Changes — "b.txt" is hidden, selection should fall back.
        state.filter = PreviewFilter::Changes;
        state.restore_selection_to_path(pinned);
        assert!(state.selected < state.sorted_filtered_indices().len());
    }

    #[test]
    fn restore_selection_finds_same_path_after_sort_change() {
        let mut state = mk_state(vec![
            copy_right("z.txt", 100),
            copy_right("a.txt", 1),
            copy_right("m.txt", 5_000),
        ]);
        state.filter = PreviewFilter::All;
        state.sort = PreviewSort::Path;
        // selected = 2 → "z.txt" by path order.
        state.selected = 2;
        let pinned = state.capture_selected_path();
        assert_eq!(pinned.as_deref(), Some(Path::new("z.txt")));

        // Switch to Size: order becomes m, z, a. "z.txt" should be at index 1.
        state.sort = PreviewSort::Size;
        state.restore_selection_to_path(pinned);
        let indices = state.sorted_filtered_indices();
        assert_eq!(
            state.actions[indices[state.selected]].path(),
            Path::new("z.txt")
        );
    }

    // --- Hierarchical Path sort -----------------------------------------

    #[test]
    fn path_sort_tree_order_with_files_before_subfolders() {
        // Input intentionally permuted; expected output is the canonical
        // tree order: root files first, then each folder with its own
        // children (files first, then subfolders) before the next sibling.
        let mut state = mk_state(vec![
            skip_dir("b"),
            skip_file("b/q.txt", 5),
            skip_file("a/sub/y.txt", 7),
            skip_dir("a/sub"),
            skip_file("a/x.txt", 3),
            skip_dir("a"),
            skip_file("z.txt", 1),
        ]);
        state.filter = PreviewFilter::All;
        state.sort = PreviewSort::Path;
        assert_eq!(
            paths_in_order(&state),
            vec![
                "z.txt",
                "a",
                "a/x.txt",
                "a/sub",
                "a/sub/y.txt",
                "b",
                "b/q.txt",
            ]
        );
    }

    #[test]
    fn path_sort_root_file_comes_before_root_dir_even_when_alphabetically_after() {
        // 'z.txt' > 'a' lexicographically, but the file should still come
        // before the dir at the same level.
        let mut state = mk_state(vec![skip_dir("a"), skip_file("z.txt", 100)]);
        state.filter = PreviewFilter::All;
        state.sort = PreviewSort::Path;
        assert_eq!(paths_in_order(&state), vec!["z.txt", "a"]);
    }

    #[test]
    fn path_sort_folder_entry_immediately_followed_by_its_contents() {
        // 'b' appears between 'a' and 'a/x.txt' if the key is naive — make
        // sure the dir 'a' hugs its content before 'b' opens.
        let mut state = mk_state(vec![
            skip_dir("b"),
            skip_dir("a"),
            skip_file("a/x.txt", 1),
            skip_file("b/q.txt", 1),
        ]);
        state.filter = PreviewFilter::All;
        state.sort = PreviewSort::Path;
        assert_eq!(paths_in_order(&state), vec!["a", "a/x.txt", "b", "b/q.txt"]);
    }

    // --- Size sort with dir aggregates and identical folders ------------

    #[test]
    fn size_sort_uses_real_skip_size_and_dir_aggregate() {
        // Mirrors the user's original bug report: both folders identical →
        // every action is Skip. Size sort must order Skip-files by their
        // real file size, with the parent dir carrying the aggregate.
        let mut state = mk_state(vec![
            skip_dir("d"),
            skip_file("d/small.txt", 100),
            skip_file("d/huge.txt", 100_000),
            skip_file("d/medium.txt", 5_000),
        ]);
        state.filter = PreviewFilter::All;
        state.sort = PreviewSort::Size;

        // Dir aggregate = 100 + 100_000 + 5_000 = 105_100, larger than any
        // single file → 'd' is at the top.
        assert_eq!(state.dir_sizes.get(Path::new("d")), Some(&105_100));
        assert_eq!(
            paths_in_order(&state),
            vec!["d", "d/huge.txt", "d/medium.txt", "d/small.txt"]
        );
    }

    #[test]
    fn manual_skip_preserves_size_so_size_sort_still_works() {
        // Convert a CopyToRight on a 1 MB file into a manual Skip with the
        // same size preserved — Size sort should still place it near the
        // top, not at the bottom with size=0.
        let mut state = mk_state(vec![
            copy_right("big.bin", 1_000_000),
            copy_right("tiny.txt", 1),
        ]);
        state.filter = PreviewFilter::All;
        state.sort = PreviewSort::Size;

        // Manually mimic the handler: keep size and is_directory.
        let big_idx = state
            .actions
            .iter()
            .position(|a| a.path() == Path::new("big.bin"))
            .unwrap();
        let big = &state.actions[big_idx];
        let preserved = UserAction::Skip {
            path: big.path().clone(),
            size: big.payload_size(),
            is_directory: big.is_directory(),
        };
        state.replace_action(big_idx, preserved);

        // After conversion the manual Skip still has size 1_000_000 and
        // therefore stays at the top of Size sort.
        assert_eq!(paths_in_order(&state), vec!["big.bin", "tiny.txt"]);
    }

    #[test]
    fn conflict_on_directory_is_recognised_as_dir_and_uses_subtree_size() {
        let mut state = mk_state(vec![
            conflict_dir("conflicted"),
            skip_file("conflicted/file.bin", 42_000),
        ]);
        state.filter = PreviewFilter::All;
        state.sort = PreviewSort::Size;

        assert!(state.actions[0].is_directory(), "conflict on dir");
        assert_eq!(
            state.dir_sizes.get(Path::new("conflicted")),
            Some(&42_000),
            "subtree aggregate populated for dir conflict"
        );
        assert_eq!(state.effective_size(&state.actions[0]), 42_000);
    }

    // --- Stable storage and replace_action ------------------------------

    #[test]
    fn selection_survives_sort_cycle() {
        // Hand-pick two distant rows by path, mark them via real indices,
        // then cycle every sort mode. The marked set must keep pointing at
        // the same UserAction entries (paths) — i.e. `actions` is never
        // reordered in place.
        let mut state = mk_state(vec![
            copy_right("alpha.txt", 1),
            copy_right("beta.txt", 1_000_000),
            copy_right("gamma.txt", 5),
            copy_right("delta.txt", 10),
        ]);
        state.filter = PreviewFilter::All;

        let alpha_idx = state
            .actions
            .iter()
            .position(|a| a.path() == Path::new("alpha.txt"))
            .unwrap();
        let beta_idx = state
            .actions
            .iter()
            .position(|a| a.path() == Path::new("beta.txt"))
            .unwrap();
        state.selected_items.insert(alpha_idx);
        state.selected_items.insert(beta_idx);

        for sort in [PreviewSort::Path, PreviewSort::Type, PreviewSort::Size] {
            state.sort = sort;
            // Force a render of the sorted view to make sure nothing
            // behind the scenes shuffles `actions`.
            let _ = state.sorted_filtered_indices();
            assert!(
                state.selected_items.contains(&alpha_idx),
                "alpha lost under sort {:?}",
                sort
            );
            assert!(
                state.selected_items.contains(&beta_idx),
                "beta lost under sort {:?}",
                sort
            );
            assert_eq!(state.actions[alpha_idx].path(), Path::new("alpha.txt"));
            assert_eq!(state.actions[beta_idx].path(), Path::new("beta.txt"));
        }
    }

    #[test]
    fn replace_action_rebuilds_dir_aggregates() {
        let mut state = mk_state(vec![
            skip_dir("d"),
            // Conflict carries the bigger side (5 MB).
            UserAction::Original(SyncAction::Conflict {
                path: PathBuf::from("d/contested.bin"),
                reason: ConflictReason::BothModified,
                left: Some(crate::sync::differ::FileInfo {
                    size: 5_000_000,
                    mtime: chrono::Utc::now(),
                    hash: None,
                    is_directory: false,
                }),
                right: Some(crate::sync::differ::FileInfo {
                    size: 1_000_000,
                    mtime: chrono::Utc::now(),
                    hash: None,
                    is_directory: false,
                }),
            }),
            skip_file("d/other.bin", 5_000_000),
        ]);
        state.filter = PreviewFilter::All;

        // Initial aggregate: max(5M, 1M) for the conflict + 5M for other.
        assert_eq!(state.dir_sizes.get(Path::new("d")), Some(&10_000_000));

        // Resolve the conflict to "copy left" with the small size 1 MB.
        let idx = state
            .actions
            .iter()
            .position(|a| a.path() == Path::new("d/contested.bin"))
            .unwrap();
        state.replace_action(
            idx,
            UserAction::CopyToLeft {
                path: PathBuf::from("d/contested.bin"),
                size: 1_000_000,
            },
        );

        // Aggregate should drop to 6_000_000 (1M + 5M).
        assert_eq!(state.dir_sizes.get(Path::new("d")), Some(&6_000_000));
    }
}
