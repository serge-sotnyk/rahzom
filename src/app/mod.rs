//! Application module

mod handlers;
pub mod state;

pub use state::{
    is_conflict_action, is_skip_action, Dialog, DialogField, DiskSpaceWarningDialog,
    ExclusionsInfoDialog, FileErrorDialog, NewProjectDialog, PreviewFilter, PreviewState,
    PreviewSummary, Screen, SyncCompleteState, SyncConfirmDialog, SyncingState, UserAction,
};

use anyhow::Result;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, ListState, Paragraph},
    Frame,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::config::project::{Project, ProjectManager};
use crate::sync::differ::{diff, SyncAction};
use crate::sync::exclusions::Exclusions;
use crate::sync::executor::{
    check_disk_space, ExecutionResult, Executor, ExecutorConfig, FailedAction, FileSnapshot,
    NoopProgress, SyncErrorKind,
};
use crate::sync::metadata::{DeletedFile, FileAttributes, FileState, SyncMetadata};
use crate::sync::scanner::scan_with_exclusions;
use crate::ui::{
    render_cancel_sync_confirm_dialog, render_create_dir_confirm_dialog,
    render_delete_confirm_dialog, render_disk_space_warning_dialog, render_error_dialog,
    render_exclusions_info_dialog, render_file_error_dialog, render_new_project_dialog,
    render_preview, render_project_list, render_project_view, render_sync_complete,
    render_sync_confirm_dialog, render_syncing,
};
use chrono::Utc;

/// Main application state
pub struct App {
    pub screen: Screen,
    pub should_quit: bool,
    pub dialog: Dialog,

    // Project list state
    pub projects: Vec<String>,
    pub list_state: ListState,
    pub project_manager: Option<ProjectManager>,

    // Current project (when in ProjectView/Preview)
    pub current_project: Option<Project>,

    // Preview state
    pub preview: Option<PreviewState>,

    // Syncing state
    pub syncing: Option<SyncingState>,

    // Sync complete state
    pub sync_complete: Option<SyncCompleteState>,

    // Exclusions state
    pub left_exclusions: Option<Exclusions>,
    pub right_exclusions: Option<Exclusions>,

    // Mouse tracking
    last_click: Option<(u16, u16, Instant)>,
    content_area: Option<Rect>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            screen: Screen::ProjectList,
            should_quit: false,
            dialog: Dialog::None,
            projects: Vec::new(),
            list_state: ListState::default(),
            project_manager: None,
            current_project: None,
            preview: None,
            syncing: None,
            sync_complete: None,
            left_exclusions: None,
            right_exclusions: None,
            last_click: None,
            content_area: None,
        };

        // Try to initialize project manager
        match ProjectManager::new() {
            Ok(pm) => {
                if let Ok(projects) = pm.list_projects() {
                    app.projects = projects;
                    if !app.projects.is_empty() {
                        app.list_state.select(Some(0));
                    }
                }
                app.project_manager = Some(pm);
            }
            Err(e) => {
                app.dialog = Dialog::Error(format!("Failed to initialize: {}", e));
            }
        }

        app
    }

    /// Create app with custom project manager (for testing)
    pub fn with_project_manager(pm: ProjectManager) -> Self {
        let projects = pm.list_projects().unwrap_or_default();
        let mut list_state = ListState::default();
        if !projects.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            screen: Screen::ProjectList,
            should_quit: false,
            dialog: Dialog::None,
            projects,
            list_state,
            project_manager: Some(pm),
            current_project: None,
            preview: None,
            syncing: None,
            sync_complete: None,
            left_exclusions: None,
            right_exclusions: None,
            last_click: None,
            content_area: None,
        }
    }

    /// Refresh project list from disk
    pub fn refresh_projects(&mut self) {
        if let Some(ref pm) = self.project_manager {
            if let Ok(projects) = pm.list_projects() {
                let was_empty = self.projects.is_empty();
                self.projects = projects;

                if self.projects.is_empty() {
                    self.list_state.select(None);
                } else if was_empty {
                    self.list_state.select(Some(0));
                } else if let Some(selected) = self.list_state.selected() {
                    if selected >= self.projects.len() {
                        self.list_state.select(Some(self.projects.len() - 1));
                    }
                }
            }
        }
    }

    /// Main application loop
    pub fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.render(frame))?;

            // If syncing and no dialog, execute one action per frame
            if self.screen == Screen::Syncing && matches!(self.dialog, Dialog::None) {
                self.execute_next_sync_action();
            }

            self.handle_events()?;
        }
        Ok(())
    }

    fn run_analyze(&mut self) {
        let Some(ref project) = self.current_project else {
            return;
        };

        // Check if paths exist
        let left_exists = project.left_path.exists();
        let right_exists = project.right_path.exists();

        if !left_exists && !right_exists {
            self.dialog = Dialog::Error(
                "At least one directory must exist. Both paths are missing.".to_string(),
            );
            return;
        }

        if !left_exists {
            self.dialog = Dialog::CreateDirConfirm {
                path: project.left_path.clone(),
                is_left: true,
            };
            return;
        }

        if !right_exists {
            self.dialog = Dialog::CreateDirConfirm {
                path: project.right_path.clone(),
                is_left: false,
            };
            return;
        }

        // Load exclusions (opt-in: returns empty if file doesn't exist)
        let left_exclusions = Exclusions::load(&project.left_path).ok();
        let right_exclusions = Exclusions::load(&project.right_path).ok();

        // Store exclusions for UI
        self.left_exclusions = left_exclusions.clone();
        self.right_exclusions = right_exclusions.clone();

        // Scan both sides with exclusions
        let left_scan = match scan_with_exclusions(&project.left_path, left_exclusions.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                self.dialog = Dialog::Error(format!("Failed to scan left: {}", e));
                return;
            }
        };

        let right_scan = match scan_with_exclusions(&project.right_path, right_exclusions.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                self.dialog = Dialog::Error(format!("Failed to scan right: {}", e));
                return;
            }
        };

        // Load metadata
        let left_meta = SyncMetadata::load(&project.left_path).unwrap_or_default();
        let right_meta = SyncMetadata::load(&project.right_path).unwrap_or_default();

        // Run diff
        let diff_result = diff(&left_scan, &right_scan, &left_meta, &right_meta);

        // Create preview state
        self.preview = Some(PreviewState::new(diff_result, left_scan, right_scan));
        self.screen = Screen::Preview;
    }

    fn show_sync_confirmation(&mut self) {
        let Some(ref preview) = self.preview else {
            return;
        };

        let summary = preview.summary();

        // Check if there's anything to sync
        let total_operations = summary.copy_to_right
            + summary.copy_to_left
            + summary.delete_right
            + summary.delete_left
            + summary.dirs_to_create;

        if total_operations == 0 {
            self.dialog = Dialog::Error("Nothing to sync - all items are skipped".to_string());
            return;
        }

        self.dialog = Dialog::SyncConfirm(SyncConfirmDialog {
            files_to_copy: summary.copy_to_right + summary.copy_to_left,
            files_to_delete: summary.delete_right + summary.delete_left,
            bytes_to_transfer: summary.bytes_to_right + summary.bytes_to_left,
            dirs_to_create: summary.dirs_to_create,
        });
    }

    fn start_sync(&mut self, skip_disk_check: bool) {
        let Some(ref preview) = self.preview else {
            return;
        };
        let Some(ref project) = self.current_project else {
            return;
        };

        // Convert UserActions to SyncActions, filtering out Skip/Conflict
        let actions: Vec<SyncAction> = preview
            .actions
            .iter()
            .filter_map(|ua| ua.to_sync_action())
            .collect();

        if actions.is_empty() {
            self.dialog = Dialog::Error("No actions to execute".to_string());
            return;
        }

        // Build snapshots from scan results for file verification
        let mut snapshots = HashMap::new();
        if let Some(ref left_scan) = preview.left_scan {
            for entry in &left_scan.entries {
                if !entry.is_dir {
                    snapshots.insert(
                        project.left_path.join(&entry.path),
                        FileSnapshot {
                            size: entry.size,
                            mtime: entry.mtime,
                        },
                    );
                }
            }
        }
        if let Some(ref right_scan) = preview.right_scan {
            for entry in &right_scan.entries {
                if !entry.is_dir {
                    snapshots.insert(
                        project.right_path.join(&entry.path),
                        FileSnapshot {
                            size: entry.size,
                            mtime: entry.mtime,
                        },
                    );
                }
            }
        }

        // Calculate bytes per direction
        let bytes_to_right: u64 = actions
            .iter()
            .map(|a| match a {
                SyncAction::CopyToRight { size, .. } => *size,
                _ => 0,
            })
            .sum();

        let bytes_to_left: u64 = actions
            .iter()
            .map(|a| match a {
                SyncAction::CopyToLeft { size, .. } => *size,
                _ => 0,
            })
            .sum();

        let total_bytes = bytes_to_right + bytes_to_left;

        // Check disk space before starting sync (unless user already confirmed)
        if !skip_disk_check {
            if bytes_to_right > 0 {
                if let Ok(info) = check_disk_space(&project.right_path, bytes_to_right) {
                    if !info.sufficient {
                        self.dialog = Dialog::DiskSpaceWarning(DiskSpaceWarningDialog {
                            is_left: false,
                            path: project.right_path.clone(),
                            available: info.available,
                            required: info.required,
                        });
                        return;
                    }
                }
            }

            if bytes_to_left > 0 {
                if let Ok(info) = check_disk_space(&project.left_path, bytes_to_left) {
                    if !info.sufficient {
                        self.dialog = Dialog::DiskSpaceWarning(DiskSpaceWarningDialog {
                            is_left: true,
                            path: project.left_path.clone(),
                            available: info.available,
                            required: info.required,
                        });
                        return;
                    }
                }
            }
        }

        self.syncing = Some(SyncingState {
            total_actions: actions.len(),
            completed_actions: 0,
            total_bytes,
            transferred_bytes: 0,
            current_file: PathBuf::new(),
            start_time: Instant::now(),
            cancel_requested: false,
            current_index: 0,
            actions,
            snapshots,
            result: ExecutionResult::default(),
        });

        self.dialog = Dialog::None;
        self.screen = Screen::Syncing;
    }

    fn execute_next_sync_action(&mut self) {
        let Some(ref project) = self.current_project else {
            return;
        };
        let Some(ref mut syncing) = self.syncing else {
            return;
        };

        // Check if cancelled
        if syncing.cancel_requested {
            self.finish_sync(true);
            return;
        }

        // Check if done
        if syncing.current_index >= syncing.actions.len() {
            self.finish_sync(false);
            return;
        }

        let action = syncing.actions[syncing.current_index].clone();

        // Update current file display
        syncing.current_file = action.path().clone();

        // Create executor for this action
        let executor = Executor::new(
            project.left_path.clone(),
            project.right_path.clone(),
            ExecutorConfig::default(),
        );

        // Execute single action
        let single_action = vec![action.clone()];
        match executor.execute(single_action, &syncing.snapshots, &mut NoopProgress) {
            Ok(result) => {
                // Check for recoverable errors that should show dialog
                if let Some(failed) = result.failed.first() {
                    if matches!(
                        failed.kind,
                        SyncErrorKind::FileLocked | SyncErrorKind::PermissionDenied
                    ) {
                        // Show error dialog - don't increment index yet
                        self.dialog = Dialog::FileError(FileErrorDialog {
                            path: failed.action.path().clone(),
                            error: failed.error.clone(),
                            kind: failed.kind.clone(),
                            action: failed.action.clone(),
                        });
                        return;
                    }
                }

                // Update progress
                syncing.transferred_bytes += result.total_bytes_transferred();

                // Accumulate results
                syncing.result.completed.extend(result.completed);
                syncing.result.failed.extend(result.failed);
                syncing.result.skipped.extend(result.skipped);
            }
            Err(e) => {
                syncing.result.failed.push(FailedAction {
                    action,
                    error: e.to_string(),
                    kind: SyncErrorKind::IoError,
                });
            }
        }

        syncing.completed_actions += 1;
        syncing.current_index += 1;
    }

    /// Skip the current sync action and move to next
    fn skip_current_sync_action(&mut self) {
        use crate::sync::executor::SkippedAction;

        let Some(ref mut syncing) = self.syncing else {
            return;
        };

        if syncing.current_index >= syncing.actions.len() {
            return;
        }

        let action = syncing.actions[syncing.current_index].clone();
        syncing.result.skipped.push(SkippedAction {
            action,
            reason: "Skipped by user".to_string(),
        });
        syncing.completed_actions += 1;
        syncing.current_index += 1;
    }

    fn finish_sync(&mut self, cancelled: bool) {
        let Some(syncing) = self.syncing.take() else {
            return;
        };

        // Calculate values before moving
        let duration = syncing.elapsed();
        let bytes_transferred = syncing.transferred_bytes;

        // Collect changed files from skipped actions
        let changed_during_sync: Vec<PathBuf> = syncing
            .result
            .skipped
            .iter()
            .filter(|s| s.reason.contains("changed"))
            .filter_map(|s| match &s.action {
                SyncAction::CopyToRight { path, .. } | SyncAction::CopyToLeft { path, .. } => {
                    Some(path.clone())
                }
                _ => None,
            })
            .collect();

        // Update metadata if sync was successful (not cancelled)
        if !cancelled {
            if let Err(e) = self.save_sync_metadata(&syncing.result) {
                // Log error but don't fail
                eprintln!("Failed to save metadata: {}", e);
            }
        }

        self.sync_complete = Some(SyncCompleteState {
            completed: syncing.result.completed,
            failed: syncing.result.failed,
            skipped: syncing.result.skipped,
            duration,
            bytes_transferred,
            scroll_offset: 0,
            changed_during_sync,
        });

        self.preview = None;
        self.screen = Screen::SyncComplete;
    }

    fn save_sync_metadata(&self, result: &ExecutionResult) -> Result<()> {
        let Some(ref project) = self.current_project else {
            return Ok(());
        };

        // Load existing metadata
        let mut left_meta = SyncMetadata::load(&project.left_path).unwrap_or_default();
        let mut right_meta = SyncMetadata::load(&project.right_path).unwrap_or_default();

        let now = Utc::now();

        // Update metadata based on completed actions
        for completed in &result.completed {
            match &completed.action {
                SyncAction::CopyToRight { path, .. } => {
                    // Read actual file metadata from disk (destination file)
                    let dest_path = project.right_path.join(path);
                    if let Ok(metadata) = std::fs::metadata(&dest_path) {
                        let mtime = metadata
                            .modified()
                            .ok()
                            .and_then(|t| chrono::DateTime::<Utc>::from(t).into())
                            .unwrap_or(now);
                        let size = metadata.len();
                        let attributes = FileAttributes::read_from_path(&dest_path);

                        let file_state = FileState {
                            path: path.to_string_lossy().to_string(),
                            size,
                            mtime,
                            hash: None,
                            attributes,
                            last_synced: now,
                        };
                        left_meta.upsert_file(file_state.clone());
                        right_meta.upsert_file(file_state);
                    }
                }
                SyncAction::CopyToLeft { path, .. } => {
                    // Read actual file metadata from disk (destination file)
                    let dest_path = project.left_path.join(path);
                    if let Ok(metadata) = std::fs::metadata(&dest_path) {
                        let mtime = metadata
                            .modified()
                            .ok()
                            .and_then(|t| chrono::DateTime::<Utc>::from(t).into())
                            .unwrap_or(now);
                        let size = metadata.len();
                        let attributes = FileAttributes::read_from_path(&dest_path);

                        let file_state = FileState {
                            path: path.to_string_lossy().to_string(),
                            size,
                            mtime,
                            hash: None,
                            attributes,
                            last_synced: now,
                        };
                        left_meta.upsert_file(file_state.clone());
                        right_meta.upsert_file(file_state);
                    }
                }
                SyncAction::DeleteRight { path } => {
                    let path_str = path.to_string_lossy().to_string();
                    right_meta.mark_deleted(DeletedFile {
                        path: path_str,
                        size: 0,
                        mtime: now,
                        hash: None,
                        deleted_at: now,
                    });
                }
                SyncAction::DeleteLeft { path } => {
                    let path_str = path.to_string_lossy().to_string();
                    left_meta.mark_deleted(DeletedFile {
                        path: path_str,
                        size: 0,
                        mtime: now,
                        hash: None,
                        deleted_at: now,
                    });
                }
                _ => {}
            }
        }

        left_meta.last_sync = Some(now);
        right_meta.last_sync = Some(now);

        left_meta.save(&project.left_path)?;
        right_meta.save(&project.right_path)?;

        Ok(())
    }

    fn try_create_project(&mut self) {
        if let Dialog::NewProject(ref dialog) = self.dialog {
            if dialog.name.is_empty() {
                if let Dialog::NewProject(ref mut d) = self.dialog {
                    d.error = Some("Project name is required".to_string());
                }
                return;
            }
            if dialog.left_path.is_empty() {
                if let Dialog::NewProject(ref mut d) = self.dialog {
                    d.error = Some("Left path is required".to_string());
                }
                return;
            }
            if dialog.right_path.is_empty() {
                if let Dialog::NewProject(ref mut d) = self.dialog {
                    d.error = Some("Right path is required".to_string());
                }
                return;
            }

            let project = Project::new(
                dialog.name.clone(),
                PathBuf::from(&dialog.left_path),
                PathBuf::from(&dialog.right_path),
            );

            if let Some(ref pm) = self.project_manager {
                match pm.save_project(&project) {
                    Ok(()) => {
                        self.dialog = Dialog::None;
                        self.refresh_projects();
                        if let Some(pos) = self.projects.iter().position(|p| p == &project.name) {
                            self.list_state.select(Some(pos));
                        }
                    }
                    Err(e) => {
                        if let Dialog::NewProject(ref mut d) = self.dialog {
                            d.error = Some(format!("{}", e));
                        }
                    }
                }
            }
        }
    }

    fn delete_project(&mut self, name: &str) {
        if let Some(ref pm) = self.project_manager {
            if let Err(e) = pm.delete_project(name) {
                self.dialog = Dialog::Error(format!("Failed to delete: {}", e));
            } else {
                self.refresh_projects();
            }
        }
    }

    fn show_exclusions_dialog(&mut self) {
        let Some(ref project) = self.current_project else {
            return;
        };

        let left_path = Exclusions::file_path(&project.left_path);
        let right_path = Exclusions::file_path(&project.right_path);
        let left_exists = left_path.exists();
        let right_exists = right_path.exists();
        let left_count = self.left_exclusions.as_ref().map(|e| e.len()).unwrap_or(0);
        let right_count = self.right_exclusions.as_ref().map(|e| e.len()).unwrap_or(0);

        self.dialog = Dialog::ExclusionsInfo(ExclusionsInfoDialog {
            left_path,
            right_path,
            left_exists,
            right_exists,
            left_count,
            right_count,
        });
    }

    fn create_exclusions_template(&mut self) {
        let Some(ref project) = self.current_project else {
            return;
        };

        let template = Exclusions::default_template();
        let left_path = Exclusions::file_path(&project.left_path);
        let right_path = Exclusions::file_path(&project.right_path);

        // Create on left side if doesn't exist
        if !left_path.exists() {
            if let Err(e) = std::fs::write(&left_path, &template) {
                self.dialog = Dialog::Error(format!("Failed to create left exclusions: {}", e));
                return;
            }
        }

        // Create on right side if doesn't exist
        if !right_path.exists() {
            if let Err(e) = std::fs::write(&right_path, &template) {
                self.dialog = Dialog::Error(format!("Failed to create right exclusions: {}", e));
                return;
            }
        }

        // Close dialog and re-run analyze to apply new exclusions
        self.dialog = Dialog::None;
        self.run_analyze();
    }

    /// Render the application
    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        let chunks = Layout::vertical([
            Constraint::Length(3), // Header
            Constraint::Min(1),    // Content
            Constraint::Length(3), // Footer
        ])
        .split(area);

        self.render_header(frame, chunks[0]);
        self.render_content(frame, chunks[1]);
        self.render_footer(frame, chunks[2]);

        match &self.dialog {
            Dialog::None => {}
            Dialog::NewProject(dialog) => {
                render_new_project_dialog(frame, dialog);
            }
            Dialog::DeleteConfirm(name) => {
                render_delete_confirm_dialog(frame, name);
            }
            Dialog::CreateDirConfirm { path, is_left } => {
                render_create_dir_confirm_dialog(frame, path, *is_left);
            }
            Dialog::Error(msg) => {
                render_error_dialog(frame, msg);
            }
            Dialog::SyncConfirm(dialog) => {
                render_sync_confirm_dialog(frame, dialog);
            }
            Dialog::CancelSyncConfirm => {
                render_cancel_sync_confirm_dialog(frame);
            }
            Dialog::ExclusionsInfo(dialog) => {
                render_exclusions_info_dialog(frame, dialog);
            }
            Dialog::DiskSpaceWarning(dialog) => {
                render_disk_space_warning_dialog(frame, dialog);
            }
            Dialog::FileError(dialog) => {
                render_file_error_dialog(frame, dialog);
            }
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let version = env!("CARGO_PKG_VERSION");
        let title = format!(" rahzom v{} ", version);

        let screen_indicator = match self.screen {
            Screen::ProjectList => "Projects".to_string(),
            Screen::ProjectView => {
                if let Some(ref p) = self.current_project {
                    p.name.clone()
                } else {
                    "Project".to_string()
                }
            }
            Screen::Analyzing => "Analyzing...".to_string(),
            Screen::Preview => {
                if let Some(ref preview) = self.preview {
                    format!("Preview [{}]", preview.filter.label())
                } else {
                    "Preview".to_string()
                }
            }
            Screen::Syncing => "Syncing...".to_string(),
            Screen::SyncComplete => "Sync Complete".to_string(),
        };

        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                title,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("— "),
            Span::styled(screen_indicator, Style::default().fg(Color::Yellow)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(header, area);
    }

    fn render_content(&mut self, frame: &mut Frame, area: Rect) {
        self.content_area = Some(area);

        match self.screen {
            Screen::ProjectList => {
                render_project_list(frame, area, &self.projects, &mut self.list_state);
            }
            Screen::ProjectView => {
                render_project_view(frame, area, self.current_project.as_ref());
            }
            Screen::Preview => {
                if let Some(ref preview) = self.preview {
                    render_preview(frame, area, preview);
                }
            }
            Screen::Syncing => {
                if let Some(ref syncing) = self.syncing {
                    render_syncing(frame, area, syncing);
                }
            }
            Screen::SyncComplete => {
                if let Some(ref complete) = self.sync_complete {
                    render_sync_complete(frame, area, complete);
                }
            }
            _ => {}
        }
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hints = match self.screen {
            Screen::ProjectList => {
                if self.projects.is_empty() {
                    vec![
                        Span::styled(" N ", Style::default().fg(Color::Black).bg(Color::Gray)),
                        Span::raw(" New  "),
                        Span::styled(" Q ", Style::default().fg(Color::Black).bg(Color::Gray)),
                        Span::raw(" Quit "),
                    ]
                } else {
                    vec![
                        Span::styled(" ↑↓ ", Style::default().fg(Color::Black).bg(Color::Gray)),
                        Span::raw(" Nav  "),
                        Span::styled(" Enter ", Style::default().fg(Color::Black).bg(Color::Gray)),
                        Span::raw(" Open  "),
                        Span::styled(" N ", Style::default().fg(Color::Black).bg(Color::Gray)),
                        Span::raw(" New  "),
                        Span::styled(" D ", Style::default().fg(Color::Black).bg(Color::Gray)),
                        Span::raw(" Del  "),
                        Span::styled(" Q ", Style::default().fg(Color::Black).bg(Color::Gray)),
                        Span::raw(" Quit "),
                    ]
                }
            }
            Screen::ProjectView => {
                vec![
                    Span::styled(" A ", Style::default().fg(Color::Black).bg(Color::Green)),
                    Span::raw(" Analyze  "),
                    Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Back  "),
                    Span::styled(" Q ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Quit "),
                ]
            }
            Screen::Preview => {
                vec![
                    Span::styled(" ↑↓ ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Nav  "),
                    Span::styled(" ←→ ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Dir  "),
                    Span::styled(" S ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Skip  "),
                    Span::styled(" G ", Style::default().fg(Color::Black).bg(Color::Green)),
                    Span::raw(" Go  "),
                    Span::styled(" E ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Excl  "),
                    Span::styled(" F ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Filter  "),
                    Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Back "),
                ]
            }
            Screen::Syncing => {
                vec![
                    Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Red)),
                    Span::raw(" Cancel "),
                ]
            }
            Screen::SyncComplete => {
                let mut hints = vec![
                    Span::styled(" Enter ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Back  "),
                ];
                if let Some(ref complete) = self.sync_complete {
                    if !complete.changed_during_sync.is_empty() {
                        hints.extend(vec![
                            Span::styled(
                                " R ",
                                Style::default().fg(Color::Black).bg(Color::Yellow),
                            ),
                            Span::raw(" Re-analyze  "),
                        ]);
                    }
                    if !complete.failed.is_empty() {
                        hints.extend(vec![
                            Span::styled(" ↑↓ ", Style::default().fg(Color::Black).bg(Color::Gray)),
                            Span::raw(" Scroll "),
                        ]);
                    }
                }
                hints
            }
            _ => vec![
                Span::styled(" Q ", Style::default().fg(Color::Black).bg(Color::Gray)),
                Span::raw(" Quit "),
            ],
        };

        let footer = Paragraph::new(Line::from(hints)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Keyboard ")
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(footer, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::differ::diff;
    use crate::sync::scanner::scan_with_exclusions;
    use crossterm::event::KeyCode;
    use tempfile::TempDir;

    fn create_test_app() -> (App, TempDir) {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let pm = ProjectManager::with_config_dir(temp.path().to_path_buf());
        let app = App::with_project_manager(pm);
        (app, temp)
    }

    #[test]
    fn test_app_initial_state() {
        let (app, _temp) = create_test_app();
        assert_eq!(app.screen, Screen::ProjectList);
        assert!(!app.should_quit);
        assert!(app.projects.is_empty());
    }

    #[test]
    fn test_quit_on_q() {
        let (mut app, _temp) = create_test_app();
        app.handle_key(KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn test_quit_on_esc() {
        let (mut app, _temp) = create_test_app();
        app.handle_key(KeyCode::Esc);
        assert!(app.should_quit);
    }

    #[test]
    fn test_open_new_project_dialog() {
        let (mut app, _temp) = create_test_app();
        app.handle_key(KeyCode::Char('n'));
        assert!(matches!(app.dialog, Dialog::NewProject(_)));
    }

    #[test]
    fn test_close_dialog_on_esc() {
        let (mut app, _temp) = create_test_app();
        app.dialog = Dialog::NewProject(NewProjectDialog::new());
        app.handle_key(KeyCode::Esc);
        assert!(matches!(app.dialog, Dialog::None));
    }

    #[test]
    fn test_new_project_dialog_tab_navigation() {
        let (mut app, _temp) = create_test_app();
        app.dialog = Dialog::NewProject(NewProjectDialog::new());

        if let Dialog::NewProject(ref d) = app.dialog {
            assert_eq!(d.focused_field, DialogField::Name);
        }

        app.handle_key(KeyCode::Tab);
        if let Dialog::NewProject(ref d) = app.dialog {
            assert_eq!(d.focused_field, DialogField::LeftPath);
        }

        app.handle_key(KeyCode::Tab);
        if let Dialog::NewProject(ref d) = app.dialog {
            assert_eq!(d.focused_field, DialogField::RightPath);
        }

        app.handle_key(KeyCode::Tab);
        if let Dialog::NewProject(ref d) = app.dialog {
            assert_eq!(d.focused_field, DialogField::Name);
        }
    }

    #[test]
    fn test_create_project() {
        let (mut app, _temp) = create_test_app();

        app.dialog = Dialog::NewProject(NewProjectDialog {
            name: "test-project".to_string(),
            left_path: "/path/left".to_string(),
            right_path: "/path/right".to_string(),
            focused_field: DialogField::Name,
            error: None,
        });

        app.try_create_project();

        assert!(matches!(app.dialog, Dialog::None));
        assert_eq!(app.projects.len(), 1);
        assert_eq!(app.projects[0], "test-project");
    }

    #[test]
    fn test_select_next_wraps() {
        let (mut app, _temp) = create_test_app();
        app.projects = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        app.list_state.select(Some(2));

        app.select_next_project();

        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_select_previous_wraps() {
        let (mut app, _temp) = create_test_app();
        app.projects = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        app.list_state.select(Some(0));

        app.select_previous_project();

        assert_eq!(app.list_state.selected(), Some(2));
    }

    #[test]
    fn test_open_project() {
        let (mut app, _temp) = create_test_app();

        let pm = app.project_manager.as_ref().unwrap();
        let project = Project::new("test", PathBuf::from("/left"), PathBuf::from("/right"));
        pm.save_project(&project).unwrap();
        app.refresh_projects();

        app.list_state.select(Some(0));
        app.open_selected_project();

        assert_eq!(app.screen, Screen::ProjectView);
        assert!(app.current_project.is_some());
    }

    #[test]
    fn test_back_from_project_view() {
        let (mut app, _temp) = create_test_app();
        app.screen = Screen::ProjectView;
        app.current_project = Some(Project::new(
            "test",
            PathBuf::from("/l"),
            PathBuf::from("/r"),
        ));

        app.handle_key(KeyCode::Esc);

        assert_eq!(app.screen, Screen::ProjectList);
        assert!(app.current_project.is_none());
    }

    #[test]
    fn test_preview_filter_cycle() {
        let filter = PreviewFilter::All;
        assert_eq!(filter.next(), PreviewFilter::Changes);
        assert_eq!(filter.next().next(), PreviewFilter::Conflicts);
        assert_eq!(filter.next().next().next(), PreviewFilter::All);
    }

    #[test]
    fn test_preview_state_creation() {
        use std::fs;

        let temp_left = TempDir::new().unwrap();
        let temp_right = TempDir::new().unwrap();

        fs::write(temp_left.path().join("file.txt"), "content").unwrap();

        let left_scan = scan_with_exclusions(temp_left.path(), None).unwrap();
        let right_scan = scan_with_exclusions(temp_right.path(), None).unwrap();
        let left_meta = SyncMetadata::default();
        let right_meta = SyncMetadata::default();

        let diff_result = diff(&left_scan, &right_scan, &left_meta, &right_meta);
        let preview = PreviewState::new(diff_result, left_scan, right_scan);

        assert!(!preview.actions.is_empty());
        assert_eq!(preview.filter, PreviewFilter::All);
        assert_eq!(preview.selected, 0);
    }

    #[test]
    fn test_analyze_both_paths_missing_shows_error() {
        let (mut app, _temp) = create_test_app();
        app.screen = Screen::ProjectView;
        app.current_project = Some(Project::new(
            "test",
            PathBuf::from("/nonexistent/left"),
            PathBuf::from("/nonexistent/right"),
        ));

        app.run_analyze();

        match &app.dialog {
            Dialog::Error(msg) => {
                assert!(msg.contains("At least one directory must exist"));
            }
            _ => panic!("Expected Error dialog"),
        }
    }

    #[test]
    fn test_analyze_left_missing_shows_create_dialog() {
        let (mut app, _temp) = create_test_app();
        let temp_right = TempDir::new().unwrap();

        app.screen = Screen::ProjectView;
        app.current_project = Some(Project::new(
            "test",
            PathBuf::from("/nonexistent/left"),
            temp_right.path().to_path_buf(),
        ));

        app.run_analyze();

        match &app.dialog {
            Dialog::CreateDirConfirm { path, is_left } => {
                assert_eq!(path, &PathBuf::from("/nonexistent/left"));
                assert!(*is_left);
            }
            _ => panic!("Expected CreateDirConfirm dialog"),
        }
    }

    #[test]
    fn test_analyze_right_missing_shows_create_dialog() {
        let (mut app, _temp) = create_test_app();
        let temp_left = TempDir::new().unwrap();

        app.screen = Screen::ProjectView;
        app.current_project = Some(Project::new(
            "test",
            temp_left.path().to_path_buf(),
            PathBuf::from("/nonexistent/right"),
        ));

        app.run_analyze();

        match &app.dialog {
            Dialog::CreateDirConfirm { path, is_left } => {
                assert_eq!(path, &PathBuf::from("/nonexistent/right"));
                assert!(!*is_left);
            }
            _ => panic!("Expected CreateDirConfirm dialog"),
        }
    }

    #[test]
    fn test_create_dir_on_confirm() {
        let (mut app, _temp) = create_test_app();
        let temp_left = TempDir::new().unwrap();
        let right_path = temp_left.path().join("new_dir");

        app.screen = Screen::ProjectView;
        app.current_project = Some(Project::new(
            "test",
            temp_left.path().to_path_buf(),
            right_path.clone(),
        ));

        app.dialog = Dialog::CreateDirConfirm {
            path: right_path.clone(),
            is_left: false,
        };

        app.handle_key(KeyCode::Char('y'));

        assert!(right_path.exists());
        // After creation, analyze runs and we should be in Preview or have scanned
        assert!(matches!(app.dialog, Dialog::None) || matches!(app.screen, Screen::Preview));
    }

    #[test]
    fn test_create_dir_cancel() {
        let (mut app, _temp) = create_test_app();

        app.dialog = Dialog::CreateDirConfirm {
            path: PathBuf::from("/some/path"),
            is_left: true,
        };

        app.handle_key(KeyCode::Char('n'));

        assert!(matches!(app.dialog, Dialog::None));
    }
}
