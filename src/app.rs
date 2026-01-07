use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::project::{Project, ProjectManager};
use crate::sync::differ::{diff, ConflictReason, DiffResult, SyncAction};
use crate::sync::executor::{
    CompletedAction, ExecutionResult, Executor, ExecutorConfig, FailedAction, FileSnapshot,
    NoopProgress, SkippedAction,
};
use crate::sync::metadata::{DeletedFile, FileAttributes, FileState, SyncMetadata};
use crate::sync::scanner::{scan, ScanResult};
use chrono::Utc;

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
    fn next(self) -> Self {
        match self {
            Self::All => Self::Changes,
            Self::Changes => Self::Conflicts,
            Self::Conflicts => Self::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Changes => "Changes",
            Self::Conflicts => "Conflicts",
        }
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
    /// User chose to skip this item
    Skip { path: PathBuf },
}

impl UserAction {
    fn path(&self) -> &PathBuf {
        match self {
            Self::Original(action) => action_path(action),
            Self::CopyToRight { path, .. } => path,
            Self::CopyToLeft { path, .. } => path,
            Self::Skip { path } => path,
        }
    }

    fn is_modified(&self) -> bool {
        !matches!(self, Self::Original(_))
    }

    /// Converts UserAction to SyncAction for execution.
    /// Returns None for Skip and Conflict actions.
    fn to_sync_action(&self) -> Option<SyncAction> {
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
            UserAction::Skip { .. } => None,
        }
    }
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
    fn new(diff_result: DiffResult, left_scan: ScanResult, right_scan: ScanResult) -> Self {
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

    fn filtered_indices(&self) -> Vec<usize> {
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

    fn summary(&self) -> PreviewSummary {
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
                UserAction::Original(SyncAction::DeleteRight { .. }) => {
                    summary.delete_right += 1;
                }
                UserAction::Original(SyncAction::DeleteLeft { .. }) => {
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
    fn get_file_size_from_left(&self, path: &Path) -> Option<u64> {
        self.left_scan
            .as_ref()?
            .entries
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.size)
    }

    /// Get file size from right scan by path
    fn get_file_size_from_right(&self, path: &Path) -> Option<u64> {
        self.right_scan
            .as_ref()?
            .entries
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.size)
    }
}

#[derive(Debug, Default)]
struct PreviewSummary {
    copy_to_right: usize,
    copy_to_left: usize,
    bytes_to_right: u64,
    bytes_to_left: u64,
    delete_right: usize,
    delete_left: usize,
    conflicts: usize,
    dirs_to_create: usize,
    skipped: usize,
}

/// Sync confirmation dialog data
#[derive(Debug, Clone, PartialEq)]
pub struct SyncConfirmDialog {
    pub files_to_copy: usize,
    pub files_to_delete: usize,
    pub bytes_to_transfer: u64,
    pub dirs_to_create: usize,
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
    fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    fn estimated_remaining(&self) -> Option<Duration> {
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
    fn new() -> Self {
        Self {
            name: String::new(),
            left_path: String::new(),
            right_path: String::new(),
            focused_field: DialogField::Name,
            error: None,
        }
    }

    fn focused_value_mut(&mut self) -> &mut String {
        match self.focused_field {
            DialogField::Name => &mut self.name,
            DialogField::LeftPath => &mut self.left_path,
            DialogField::RightPath => &mut self.right_path,
        }
    }

    fn next_field(&mut self) {
        self.focused_field = match self.focused_field {
            DialogField::Name => DialogField::LeftPath,
            DialogField::LeftPath => DialogField::RightPath,
            DialogField::RightPath => DialogField::Name,
        };
    }

    fn prev_field(&mut self) {
        self.focused_field = match self.focused_field {
            DialogField::Name => DialogField::RightPath,
            DialogField::LeftPath => DialogField::Name,
            DialogField::RightPath => DialogField::LeftPath,
        };
    }
}

/// Dialog input fields
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogField {
    Name,
    LeftPath,
    RightPath,
}

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

    /// Handle input events
    fn handle_events(&mut self) -> Result<()> {
        // Use shorter poll timeout during sync for responsiveness
        let poll_timeout = if self.screen == Screen::Syncing {
            Duration::from_millis(10)
        } else {
            Duration::from_millis(100)
        };

        if event::poll(poll_timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    self.handle_key(key.code);
                }
                Event::Mouse(mouse) => {
                    self.handle_mouse(mouse);
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        Ok(())
    }

    /// Handle keyboard input
    fn handle_key(&mut self, code: KeyCode) {
        match &self.dialog {
            Dialog::None => self.handle_key_normal(code),
            Dialog::NewProject(_) => self.handle_key_new_project(code),
            Dialog::DeleteConfirm(_) => self.handle_key_delete_confirm(code),
            Dialog::CreateDirConfirm { .. } => self.handle_key_create_dir_confirm(code),
            Dialog::Error(_) => self.handle_key_error(code),
            Dialog::SyncConfirm(_) => self.handle_key_sync_confirm(code),
            Dialog::CancelSyncConfirm => self.handle_key_cancel_sync_confirm(code),
        }
    }

    fn handle_key_normal(&mut self, code: KeyCode) {
        match self.screen {
            Screen::ProjectList => self.handle_key_project_list(code),
            Screen::ProjectView => self.handle_key_project_view(code),
            Screen::Preview => self.handle_key_preview(code),
            Screen::Syncing => self.handle_key_syncing(code),
            Screen::SyncComplete => self.handle_key_sync_complete(code),
            _ => {}
        }
    }

    fn handle_key_project_list(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.should_quit = true;
            }
            KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_previous_project();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next_project();
            }
            KeyCode::Enter => {
                self.open_selected_project();
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.dialog = Dialog::NewProject(NewProjectDialog::new());
            }
            KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Delete => {
                if let Some(selected) = self.list_state.selected() {
                    if let Some(name) = self.projects.get(selected) {
                        self.dialog = Dialog::DeleteConfirm(name.clone());
                    }
                }
            }
            KeyCode::Home => {
                if !self.projects.is_empty() {
                    self.list_state.select(Some(0));
                }
            }
            KeyCode::End => {
                if !self.projects.is_empty() {
                    self.list_state.select(Some(self.projects.len() - 1));
                }
            }
            _ => {}
        }
    }

    fn handle_key_project_view(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc | KeyCode::Backspace => {
                self.screen = Screen::ProjectList;
                self.current_project = None;
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.should_quit = true;
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.run_analyze();
            }
            _ => {}
        }
    }

    fn handle_key_preview(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc | KeyCode::Backspace => {
                self.screen = Screen::ProjectView;
                self.preview = None;
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.should_quit = true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_previous_action();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next_action();
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                self.cycle_filter();
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.change_action_to_left();
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.change_action_to_right();
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                self.skip_selected_action();
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.reset_selected_action();
            }
            KeyCode::Char('g') | KeyCode::Char('G') => {
                self.show_sync_confirmation();
            }
            KeyCode::Char(' ') => {
                self.toggle_selection();
            }
            KeyCode::Home => {
                if let Some(ref mut preview) = self.preview {
                    let indices = preview.filtered_indices();
                    if !indices.is_empty() {
                        preview.selected = 0;
                        preview.scroll_offset = 0;
                    }
                }
            }
            KeyCode::End => {
                if let Some(ref mut preview) = self.preview {
                    let indices = preview.filtered_indices();
                    if !indices.is_empty() {
                        preview.selected = indices.len() - 1;
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_key_new_project(&mut self, code: KeyCode) {
        if let Dialog::NewProject(ref mut dialog) = self.dialog {
            match code {
                KeyCode::Esc => {
                    self.dialog = Dialog::None;
                }
                KeyCode::Tab => {
                    dialog.next_field();
                }
                KeyCode::BackTab => {
                    dialog.prev_field();
                }
                KeyCode::Enter => {
                    self.try_create_project();
                }
                KeyCode::Backspace => {
                    dialog.focused_value_mut().pop();
                    dialog.error = None;
                }
                KeyCode::Char(c) => {
                    dialog.focused_value_mut().push(c);
                    dialog.error = None;
                }
                _ => {}
            }
        }
    }

    fn handle_key_delete_confirm(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if let Dialog::DeleteConfirm(ref name) = self.dialog {
                    let name = name.clone();
                    self.delete_project(&name);
                }
                self.dialog = Dialog::None;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.dialog = Dialog::None;
            }
            _ => {}
        }
    }

    fn handle_key_create_dir_confirm(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if let Dialog::CreateDirConfirm { ref path, .. } = self.dialog {
                    let path = path.clone();
                    match std::fs::create_dir_all(&path) {
                        Ok(()) => {
                            self.dialog = Dialog::None;
                            self.run_analyze();
                        }
                        Err(e) => {
                            self.dialog =
                                Dialog::Error(format!("Failed to create directory: {}", e));
                        }
                    }
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.dialog = Dialog::None;
            }
            _ => {}
        }
    }

    fn handle_key_error(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Esc => {
                self.dialog = Dialog::None;
            }
            _ => {}
        }
    }

    fn handle_key_sync_confirm(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter => {
                self.dialog = Dialog::None;
                self.start_sync();
            }
            KeyCode::Esc => {
                self.dialog = Dialog::None;
            }
            _ => {}
        }
    }

    fn handle_key_cancel_sync_confirm(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if let Some(ref mut syncing) = self.syncing {
                    syncing.cancel_requested = true;
                }
                self.dialog = Dialog::None;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.dialog = Dialog::None;
            }
            _ => {}
        }
    }

    fn handle_key_syncing(&mut self, code: KeyCode) {
        if code == KeyCode::Esc {
            self.dialog = Dialog::CancelSyncConfirm;
        }
    }

    fn handle_key_sync_complete(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Esc => {
                self.sync_complete = None;
                self.screen = Screen::ProjectView;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if let Some(ref complete) = self.sync_complete {
                    if !complete.changed_during_sync.is_empty() {
                        self.sync_complete = None;
                        self.run_analyze();
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(ref mut complete) = self.sync_complete {
                    if complete.scroll_offset > 0 {
                        complete.scroll_offset -= 1;
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(ref mut complete) = self.sync_complete {
                    let max_scroll = complete.failed.len().saturating_sub(1);
                    if complete.scroll_offset < max_scroll {
                        complete.scroll_offset += 1;
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle mouse input
    fn handle_mouse(&mut self, mouse: event::MouseEvent) {
        if !matches!(self.dialog, Dialog::None) {
            return;
        }

        match self.screen {
            Screen::ProjectList => self.handle_mouse_project_list(mouse),
            Screen::Preview => self.handle_mouse_preview(mouse),
            _ => {}
        }
    }

    fn handle_mouse_project_list(&mut self, mouse: event::MouseEvent) {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let now = Instant::now();
                let pos = (mouse.column, mouse.row);

                if let Some((last_col, last_row, last_time)) = self.last_click {
                    if last_col == pos.0
                        && last_row == pos.1
                        && now.duration_since(last_time) < Duration::from_millis(500)
                    {
                        self.open_selected_project();
                        self.last_click = None;
                        return;
                    }
                }

                if let Some(content_area) = self.content_area {
                    if mouse.column >= content_area.x
                        && mouse.column < content_area.x + content_area.width
                        && mouse.row >= content_area.y
                        && mouse.row < content_area.y + content_area.height
                    {
                        let relative_y = mouse.row.saturating_sub(content_area.y + 1);
                        let index = relative_y as usize;

                        if index < self.projects.len() {
                            self.list_state.select(Some(index));
                        }
                    }
                }

                self.last_click = Some((pos.0, pos.1, now));
            }
            MouseEventKind::ScrollUp => {
                self.select_previous_project();
            }
            MouseEventKind::ScrollDown => {
                self.select_next_project();
            }
            _ => {}
        }
    }

    fn handle_mouse_preview(&mut self, mouse: event::MouseEvent) {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(content_area) = self.content_area {
                    if mouse.column >= content_area.x
                        && mouse.column < content_area.x + content_area.width
                        && mouse.row >= content_area.y
                        && mouse.row < content_area.y + content_area.height
                    {
                        if let Some(ref mut preview) = self.preview {
                            let relative_y = mouse.row.saturating_sub(content_area.y + 1);
                            let index = relative_y as usize + preview.scroll_offset;
                            let indices = preview.filtered_indices();

                            if index < indices.len() {
                                preview.selected = index;
                            }
                        }
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                self.select_previous_action();
            }
            MouseEventKind::ScrollDown => {
                self.select_next_action();
            }
            _ => {}
        }
    }

    // Navigation helpers
    fn select_next_project(&mut self) {
        if self.projects.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.projects.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn select_previous_project(&mut self) {
        if self.projects.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.projects.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn select_next_action(&mut self) {
        if let Some(ref mut preview) = self.preview {
            let indices = preview.filtered_indices();
            if !indices.is_empty() && preview.selected < indices.len() - 1 {
                preview.selected += 1;
            }
        }
    }

    fn select_previous_action(&mut self) {
        if let Some(ref mut preview) = self.preview {
            if preview.selected > 0 {
                preview.selected -= 1;
            }
        }
    }

    fn cycle_filter(&mut self) {
        if let Some(ref mut preview) = self.preview {
            preview.filter = preview.filter.next();
            preview.selected = 0;
            preview.scroll_offset = 0;
        }
    }

    fn toggle_selection(&mut self) {
        if let Some(ref mut preview) = self.preview {
            let indices = preview.filtered_indices();
            if let Some(&real_idx) = indices.get(preview.selected) {
                if preview.selected_items.contains(&real_idx) {
                    preview.selected_items.remove(&real_idx);
                } else {
                    preview.selected_items.insert(real_idx);
                }
            }
        }
    }

    fn change_action_to_left(&mut self) {
        if let Some(ref mut preview) = self.preview {
            let indices = preview.filtered_indices();
            if let Some(&real_idx) = indices.get(preview.selected) {
                if let Some(action) = preview.actions.get(real_idx) {
                    let path = action.path().clone();
                    // CopyToLeft means source is RIGHT side - get size from right_scan
                    let size = preview.get_file_size_from_right(&path).unwrap_or(0);
                    preview.actions[real_idx] = UserAction::CopyToLeft { path, size };
                }
            }
        }
    }

    fn change_action_to_right(&mut self) {
        if let Some(ref mut preview) = self.preview {
            let indices = preview.filtered_indices();
            if let Some(&real_idx) = indices.get(preview.selected) {
                if let Some(action) = preview.actions.get(real_idx) {
                    let path = action.path().clone();
                    // CopyToRight means source is LEFT side - get size from left_scan
                    let size = preview.get_file_size_from_left(&path).unwrap_or(0);
                    preview.actions[real_idx] = UserAction::CopyToRight { path, size };
                }
            }
        }
    }

    fn skip_selected_action(&mut self) {
        if let Some(ref mut preview) = self.preview {
            let indices = preview.filtered_indices();
            if let Some(&real_idx) = indices.get(preview.selected) {
                if let Some(action) = preview.actions.get(real_idx) {
                    let path = action.path().clone();
                    preview.actions[real_idx] = UserAction::Skip { path };
                }
            }
        }
    }

    fn reset_selected_action(&mut self) {
        if let Some(ref mut preview) = self.preview {
            let indices = preview.filtered_indices();
            if let Some(&_real_idx) = indices.get(preview.selected) {
                // We need to restore the original action - but we don't have it stored separately
                // For now, action reset is not fully implemented
                // In a full implementation, we'd store original DiffResult
            }
        }
    }

    fn open_selected_project(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if let Some(name) = self.projects.get(selected) {
                if let Some(ref pm) = self.project_manager {
                    match pm.load_project(name) {
                        Ok(project) => {
                            self.current_project = Some(project);
                            self.screen = Screen::ProjectView;
                        }
                        Err(e) => {
                            self.dialog = Dialog::Error(format!("Failed to load project: {}", e));
                        }
                    }
                }
            }
        }
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

        // Scan both sides
        let left_scan = match scan(&project.left_path) {
            Ok(s) => s,
            Err(e) => {
                self.dialog = Dialog::Error(format!("Failed to scan left: {}", e));
                return;
            }
        };

        let right_scan = match scan(&project.right_path) {
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

    fn start_sync(&mut self) {
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

        // Calculate total bytes
        let total_bytes: u64 = actions
            .iter()
            .map(|a| match a {
                SyncAction::CopyToRight { size, .. } | SyncAction::CopyToLeft { size, .. } => *size,
                _ => 0,
            })
            .sum();

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
        syncing.current_file = match &action {
            SyncAction::CopyToRight { path, .. }
            | SyncAction::CopyToLeft { path, .. }
            | SyncAction::DeleteRight { path }
            | SyncAction::DeleteLeft { path }
            | SyncAction::CreateDirRight { path }
            | SyncAction::CreateDirLeft { path }
            | SyncAction::Skip { path, .. }
            | SyncAction::Conflict { path, .. } => path.clone(),
        };

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
                // Update progress (must be done before moving fields)
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
                });
            }
        }

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

                        let file_state = FileState {
                            path: path.to_string_lossy().to_string(),
                            size,
                            mtime,
                            hash: None,
                            attributes: FileAttributes::default(),
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

                        let file_state = FileState {
                            path: path.to_string_lossy().to_string(),
                            size,
                            mtime,
                            hash: None,
                            attributes: FileAttributes::default(),
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
                self.render_new_project_dialog(frame, dialog.clone());
            }
            Dialog::DeleteConfirm(name) => {
                self.render_delete_confirm_dialog(frame, name);
            }
            Dialog::CreateDirConfirm { path, is_left } => {
                self.render_create_dir_confirm_dialog(frame, path, *is_left);
            }
            Dialog::Error(msg) => {
                self.render_error_dialog(frame, msg);
            }
            Dialog::SyncConfirm(dialog) => {
                self.render_sync_confirm_dialog(frame, dialog);
            }
            Dialog::CancelSyncConfirm => {
                self.render_cancel_sync_confirm_dialog(frame);
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
            Screen::ProjectList => self.render_project_list(frame, area),
            Screen::ProjectView => self.render_project_view(frame, area),
            Screen::Preview => self.render_preview(frame, area),
            Screen::Syncing => self.render_syncing(frame, area),
            Screen::SyncComplete => self.render_sync_complete(frame, area),
            _ => {}
        }
    }

    fn render_project_list(&mut self, frame: &mut Frame, area: Rect) {
        if self.projects.is_empty() {
            let empty_msg = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No projects configured",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::raw("Press "),
                    Span::styled(" N ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" to create a new project"),
                ]),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Projects ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
            frame.render_widget(empty_msg, area);
            return;
        }

        let items: Vec<ListItem> = self
            .projects
            .iter()
            .map(|name| ListItem::new(Line::from(format!("  {}  ", name))))
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Projects ({}) ", self.projects.len()))
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_project_view(&self, frame: &mut Frame, area: Rect) {
        let content = if let Some(ref project) = self.current_project {
            vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("Name: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&project.name, Style::default().fg(Color::White)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Left:  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        project.left_path.display().to_string(),
                        Style::default().fg(Color::Cyan),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Right: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        project.right_path.display().to_string(),
                        Style::default().fg(Color::Cyan),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::raw("Press "),
                    Span::styled(" A ", Style::default().fg(Color::Black).bg(Color::Green)),
                    Span::raw(" to analyze"),
                ]),
            ]
        } else {
            vec![Line::from("No project loaded")]
        };

        let paragraph = Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Project Details ")
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(paragraph, area);
    }

    fn render_preview(&mut self, frame: &mut Frame, area: Rect) {
        let Some(ref preview) = self.preview else {
            return;
        };

        // Split area for list and summary
        let chunks = Layout::vertical([
            Constraint::Min(5),    // Action list
            Constraint::Length(4), // Summary
        ])
        .split(area);

        // Render action list
        let indices = preview.filtered_indices();
        let visible_height = chunks[0].height.saturating_sub(2) as usize;

        // Adjust scroll offset
        let scroll_offset = if preview.selected >= visible_height {
            preview.selected - visible_height + 1
        } else {
            0
        };

        let items: Vec<ListItem> = indices
            .iter()
            .skip(scroll_offset)
            .take(visible_height)
            .enumerate()
            .map(|(display_idx, &real_idx)| {
                let action = &preview.actions[real_idx];
                let is_selected = display_idx + scroll_offset == preview.selected;
                let is_marked = preview.selected_items.contains(&real_idx);

                render_action_item(action, is_selected, is_marked)
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    " Actions ({}/{}) ",
                    indices.len(),
                    preview.actions.len()
                ))
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(list, chunks[0]);

        // Render scrollbar if needed
        if indices.len() > visible_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None);
            let mut scrollbar_state = ScrollbarState::new(indices.len()).position(preview.selected);
            frame.render_stateful_widget(
                scrollbar,
                chunks[0].inner(Margin::new(0, 1)),
                &mut scrollbar_state,
            );
        }

        // Render summary
        let summary = preview.summary();
        self.render_summary(frame, chunks[1], &summary);
    }

    fn render_summary(&self, frame: &mut Frame, area: Rect, summary: &PreviewSummary) {
        let total_bytes = summary.bytes_to_right + summary.bytes_to_left;

        let lines = vec![
            Line::from(vec![
                Span::styled("→ ", Style::default().fg(Color::Green)),
                Span::raw(format!("{} files ", summary.copy_to_right)),
                Span::styled("← ", Style::default().fg(Color::Blue)),
                Span::raw(format!("{} files ", summary.copy_to_left)),
                Span::styled("✕ ", Style::default().fg(Color::Red)),
                Span::raw(format!(
                    "{} del ",
                    summary.delete_left + summary.delete_right
                )),
                Span::styled("⚠ ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("{} conflicts", summary.conflicts)),
            ]),
            Line::from(vec![
                Span::styled("Total: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format_bytes(total_bytes)),
                Span::raw("  "),
                Span::styled("Dirs: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format!("{}", summary.dirs_to_create)),
                Span::raw("  "),
                Span::styled("Skip: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format!("{}", summary.skipped)),
            ]),
        ];

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Summary ")
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(paragraph, area);
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
                    Span::raw(" Go/Sync  "),
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

    fn render_new_project_dialog(&self, frame: &mut Frame, dialog: NewProjectDialog) {
        let area = centered_rect(60, 14, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" New Project ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner.inner(Margin::new(2, 0)));

        let name_style = if dialog.focused_field == DialogField::Name {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let name_label = Line::from(vec![
            Span::styled("Name: ", name_style),
            Span::raw(&dialog.name),
            if dialog.focused_field == DialogField::Name {
                Span::styled("▌", Style::default().fg(Color::White))
            } else {
                Span::raw("")
            },
        ]);
        frame.render_widget(Paragraph::new(name_label), chunks[1]);

        let left_style = if dialog.focused_field == DialogField::LeftPath {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let left_label = Line::from(vec![
            Span::styled("Left path: ", left_style),
            Span::raw(&dialog.left_path),
            if dialog.focused_field == DialogField::LeftPath {
                Span::styled("▌", Style::default().fg(Color::White))
            } else {
                Span::raw("")
            },
        ]);
        frame.render_widget(Paragraph::new(left_label), chunks[3]);

        let right_style = if dialog.focused_field == DialogField::RightPath {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let right_label = Line::from(vec![
            Span::styled("Right path: ", right_style),
            Span::raw(&dialog.right_path),
            if dialog.focused_field == DialogField::RightPath {
                Span::styled("▌", Style::default().fg(Color::White))
            } else {
                Span::raw("")
            },
        ]);
        frame.render_widget(Paragraph::new(right_label), chunks[5]);

        let hint = if let Some(ref error) = dialog.error {
            Line::from(Span::styled(error, Style::default().fg(Color::Red)))
        } else {
            Line::from(vec![
                Span::styled(" Tab ", Style::default().fg(Color::Black).bg(Color::Gray)),
                Span::raw(" Next  "),
                Span::styled(" Enter ", Style::default().fg(Color::Black).bg(Color::Gray)),
                Span::raw(" Create  "),
                Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
                Span::raw(" Cancel"),
            ])
        };
        frame.render_widget(Paragraph::new(hint), chunks[7]);
    }

    fn render_delete_confirm_dialog(&self, frame: &mut Frame, name: &str) {
        let area = centered_rect(50, 7, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Confirm Delete ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let text = vec![
            Line::from(""),
            Line::from(format!("Delete project '{}'?", name)),
            Line::from(""),
            Line::from(vec![
                Span::styled(" Y ", Style::default().fg(Color::Black).bg(Color::Red)),
                Span::raw(" Yes  "),
                Span::styled(" N ", Style::default().fg(Color::Black).bg(Color::Gray)),
                Span::raw(" No"),
            ]),
        ];

        frame.render_widget(
            Paragraph::new(text).alignment(ratatui::layout::Alignment::Center),
            inner,
        );
    }

    fn render_create_dir_confirm_dialog(&self, frame: &mut Frame, path: &Path, is_left: bool) {
        let area = centered_rect(70, 9, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Create Directory ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let side = if is_left { "Left" } else { "Right" };
        let text = vec![
            Line::from(""),
            Line::from(format!("{} directory doesn't exist:", side)),
            Line::from(Span::styled(
                path.display().to_string(),
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from("Create it?"),
            Line::from(""),
            Line::from(vec![
                Span::styled(" Y ", Style::default().fg(Color::Black).bg(Color::Green)),
                Span::raw(" Yes  "),
                Span::styled(" N ", Style::default().fg(Color::Black).bg(Color::Gray)),
                Span::raw(" No"),
            ]),
        ];

        frame.render_widget(
            Paragraph::new(text).alignment(ratatui::layout::Alignment::Center),
            inner,
        );
    }

    fn render_error_dialog(&self, frame: &mut Frame, message: &str) {
        let area = centered_rect(60, 7, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Error ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let text = vec![
            Line::from(""),
            Line::from(Span::styled(message, Style::default().fg(Color::Red))),
            Line::from(""),
            Line::from(vec![
                Span::styled(" Enter ", Style::default().fg(Color::Black).bg(Color::Gray)),
                Span::raw(" OK"),
            ]),
        ];

        frame.render_widget(
            Paragraph::new(text).alignment(ratatui::layout::Alignment::Center),
            inner,
        );
    }

    fn render_sync_confirm_dialog(&self, frame: &mut Frame, dialog: &SyncConfirmDialog) {
        let area = centered_rect(60, 11, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Confirm Sync ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let text = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("Copy: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} files", dialog.files_to_copy),
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(vec![
                Span::styled("Delete: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} files", dialog.files_to_delete),
                    Style::default().fg(Color::Red),
                ),
            ]),
            Line::from(vec![
                Span::styled("Transfer: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format_bytes(dialog.bytes_to_transfer)),
            ]),
            Line::from(vec![
                Span::styled("Create dirs: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format!("{}", dialog.dirs_to_create)),
            ]),
            Line::from(""),
            Line::from("Start synchronization?"),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    " Enter ",
                    Style::default().fg(Color::Black).bg(Color::Green),
                ),
                Span::raw(" Start  "),
                Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
                Span::raw(" Cancel"),
            ]),
        ];

        frame.render_widget(
            Paragraph::new(text).alignment(ratatui::layout::Alignment::Center),
            inner,
        );
    }

    fn render_cancel_sync_confirm_dialog(&self, frame: &mut Frame) {
        let area = centered_rect(50, 7, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Cancel Sync? ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let text = vec![
            Line::from(""),
            Line::from("Cancel synchronization?"),
            Line::from(""),
            Line::from(vec![
                Span::styled(" Y ", Style::default().fg(Color::Black).bg(Color::Red)),
                Span::raw(" Yes  "),
                Span::styled(" N ", Style::default().fg(Color::Black).bg(Color::Gray)),
                Span::raw(" No"),
            ]),
        ];

        frame.render_widget(
            Paragraph::new(text).alignment(ratatui::layout::Alignment::Center),
            inner,
        );
    }

    fn render_syncing(&self, frame: &mut Frame, area: Rect) {
        let Some(ref syncing) = self.syncing else {
            return;
        };

        let chunks = Layout::vertical([
            Constraint::Length(3), // Files progress
            Constraint::Length(3), // Bytes progress
            Constraint::Length(2), // Current file
            Constraint::Length(2), // Time info
            Constraint::Min(1),    // Spacer
        ])
        .split(area);

        // Files progress bar
        let files_progress = syncing.completed_actions as f64 / syncing.total_actions.max(1) as f64;
        let files_gauge = Gauge::default()
            .block(
                Block::default()
                    .title(format!(
                        " Files: {}/{} ",
                        syncing.completed_actions, syncing.total_actions
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .gauge_style(Style::default().fg(Color::Green))
            .ratio(files_progress);
        frame.render_widget(files_gauge, chunks[0]);

        // Bytes progress bar
        let bytes_progress = if syncing.total_bytes > 0 {
            syncing.transferred_bytes as f64 / syncing.total_bytes as f64
        } else {
            1.0
        };
        let bytes_gauge = Gauge::default()
            .block(
                Block::default()
                    .title(format!(
                        " Transferred: {} / {} ",
                        format_bytes(syncing.transferred_bytes),
                        format_bytes(syncing.total_bytes)
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .gauge_style(Style::default().fg(Color::Cyan))
            .ratio(bytes_progress);
        frame.render_widget(bytes_gauge, chunks[1]);

        // Current file
        let current_file = Paragraph::new(Line::from(vec![
            Span::styled("Current: ", Style::default().fg(Color::DarkGray)),
            Span::raw(syncing.current_file.display().to_string()),
        ]));
        frame.render_widget(current_file, chunks[2]);

        // Time info
        let elapsed = format_duration(syncing.elapsed());
        let remaining = syncing
            .estimated_remaining()
            .map(format_duration)
            .unwrap_or_else(|| "calculating...".to_string());

        let time_info = Paragraph::new(Line::from(vec![
            Span::styled("Elapsed: ", Style::default().fg(Color::DarkGray)),
            Span::raw(&elapsed),
            Span::raw("  "),
            Span::styled("Remaining: ", Style::default().fg(Color::DarkGray)),
            Span::raw(&remaining),
        ]));
        frame.render_widget(time_info, chunks[3]);
    }

    fn render_sync_complete(&self, frame: &mut Frame, area: Rect) {
        let Some(ref complete) = self.sync_complete else {
            return;
        };

        let has_errors = !complete.failed.is_empty();
        let has_changed = !complete.changed_during_sync.is_empty();

        let chunks = Layout::vertical([
            Constraint::Length(7), // Summary
            if has_errors {
                Constraint::Min(5)
            } else {
                Constraint::Length(0)
            }, // Errors list
            if has_changed {
                Constraint::Length(3)
            } else {
                Constraint::Length(0)
            }, // Changed files notice
        ])
        .split(area);

        // Summary
        let summary_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("Completed: ", Style::default().fg(Color::Green)),
                Span::raw(format!("{} actions", complete.completed.len())),
            ]),
            Line::from(vec![
                Span::styled("Failed: ", Style::default().fg(Color::Red)),
                Span::raw(format!("{} actions", complete.failed.len())),
            ]),
            Line::from(vec![
                Span::styled("Skipped: ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("{} actions", complete.skipped.len())),
            ]),
            Line::from(vec![
                Span::styled("Time: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format_duration(complete.duration)),
                Span::raw("  "),
                Span::styled("Transferred: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format_bytes(complete.bytes_transferred)),
            ]),
        ];

        let summary = Paragraph::new(summary_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Sync Complete ")
                .border_style(Style::default().fg(Color::Green)),
        );
        frame.render_widget(summary, chunks[0]);

        // Errors list
        if has_errors {
            let visible_height = chunks[1].height.saturating_sub(2) as usize;
            let error_items: Vec<ListItem> = complete
                .failed
                .iter()
                .skip(complete.scroll_offset)
                .take(visible_height)
                .map(|f| {
                    let path = match &f.action {
                        SyncAction::CopyToRight { path, .. }
                        | SyncAction::CopyToLeft { path, .. }
                        | SyncAction::DeleteRight { path }
                        | SyncAction::DeleteLeft { path } => path.display().to_string(),
                        _ => "unknown".to_string(),
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled("✗ ", Style::default().fg(Color::Red)),
                        Span::raw(path),
                        Span::styled(" - ", Style::default().fg(Color::DarkGray)),
                        Span::raw(&f.error),
                    ]))
                })
                .collect();

            let errors_list = List::new(error_items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Errors ({}) ", complete.failed.len()))
                    .border_style(Style::default().fg(Color::Red)),
            );
            frame.render_widget(errors_list, chunks[1]);
        }

        // Changed files notice
        if has_changed {
            let notice = Paragraph::new(Line::from(vec![
                Span::styled("⚠ ", Style::default().fg(Color::Yellow)),
                Span::raw(format!(
                    "{} files changed during sync. Press ",
                    complete.changed_during_sync.len()
                )),
                Span::styled(" R ", Style::default().fg(Color::Black).bg(Color::Yellow)),
                Span::raw(" to re-analyze."),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            );
            frame.render_widget(notice, chunks[2]);
        }
    }
}

// Helper functions

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let popup_width = area.width * percent_x / 100;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;

    Rect::new(
        area.x + x,
        area.y + y,
        popup_width.min(area.width),
        height.min(area.height),
    )
}

fn action_path(action: &SyncAction) -> &PathBuf {
    match action {
        SyncAction::CopyToRight { path, .. } => path,
        SyncAction::CopyToLeft { path, .. } => path,
        SyncAction::DeleteRight { path } => path,
        SyncAction::DeleteLeft { path } => path,
        SyncAction::CreateDirRight { path } => path,
        SyncAction::CreateDirLeft { path } => path,
        SyncAction::Conflict { path, .. } => path,
        SyncAction::Skip { path, .. } => path,
    }
}

fn is_skip_action(action: &UserAction) -> bool {
    matches!(
        action,
        UserAction::Skip { .. } | UserAction::Original(SyncAction::Skip { .. })
    )
}

fn is_conflict_action(action: &UserAction) -> bool {
    matches!(action, UserAction::Original(SyncAction::Conflict { .. }))
}

fn render_action_item(
    action: &UserAction,
    is_selected: bool,
    is_marked: bool,
) -> ListItem<'static> {
    let (symbol, color, path_str) = match action {
        UserAction::Original(SyncAction::CopyToRight { path, size }) => (
            "→",
            Color::Green,
            format!("{} ({})", path.display(), format_bytes(*size)),
        ),
        UserAction::Original(SyncAction::CopyToLeft { path, size }) => (
            "←",
            Color::Blue,
            format!("{} ({})", path.display(), format_bytes(*size)),
        ),
        UserAction::Original(SyncAction::DeleteRight { path }) => {
            ("✕→", Color::Red, path.display().to_string())
        }
        UserAction::Original(SyncAction::DeleteLeft { path }) => {
            ("←✕", Color::Red, path.display().to_string())
        }
        UserAction::Original(SyncAction::CreateDirRight { path }) => {
            ("📁→", Color::Green, path.display().to_string())
        }
        UserAction::Original(SyncAction::CreateDirLeft { path }) => {
            ("←📁", Color::Blue, path.display().to_string())
        }
        UserAction::Original(SyncAction::Conflict { path, reason, .. }) => {
            let reason_str = match reason {
                ConflictReason::BothModified => "both modified",
                ConflictReason::ModifiedAndDeleted => "mod vs del",
                ConflictReason::ExistsVsDeleted => "exists vs del",
            };
            (
                "⚠",
                Color::Yellow,
                format!("{} ({})", path.display(), reason_str),
            )
        }
        UserAction::Original(SyncAction::Skip { path, .. }) => {
            ("·", Color::DarkGray, path.display().to_string())
        }
        UserAction::CopyToRight { path, size } => (
            "→*",
            Color::Green,
            format!("{} ({})", path.display(), format_bytes(*size)),
        ),
        UserAction::CopyToLeft { path, size } => (
            "←*",
            Color::Blue,
            format!("{} ({})", path.display(), format_bytes(*size)),
        ),
        UserAction::Skip { path } => ("·*", Color::DarkGray, path.display().to_string()),
    };

    let marker = if is_marked { "● " } else { "  " };
    let modified_indicator = if action.is_modified() { "*" } else { "" };

    let style = if is_selected {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default()
    };

    ListItem::new(Line::from(vec![
        Span::raw(marker),
        Span::styled(format!("{:<3}", symbol), Style::default().fg(color)),
        Span::raw(" "),
        Span::styled(path_str, style),
        Span::styled(modified_indicator, Style::default().fg(Color::Magenta)),
    ]))
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1073741824), "1.0 GB");
    }

    #[test]
    fn test_preview_state_creation() {
        use std::fs;

        let temp_left = TempDir::new().unwrap();
        let temp_right = TempDir::new().unwrap();

        fs::write(temp_left.path().join("file.txt"), "content").unwrap();

        let left_scan = scan(temp_left.path()).unwrap();
        let right_scan = scan(temp_right.path()).unwrap();
        let left_meta = SyncMetadata::default();
        let right_meta = SyncMetadata::default();

        let diff_result = diff(&left_scan, &right_scan, &left_meta, &right_meta);
        let preview = PreviewState::new(diff_result, left_scan, right_scan);

        assert!(!preview.actions.is_empty());
        assert_eq!(preview.filter, PreviewFilter::All);
        assert_eq!(preview.selected, 0);
    }

    #[test]
    fn test_centered_rect() {
        let area = Rect::new(0, 0, 100, 50);
        let centered = centered_rect(50, 10, area);

        assert_eq!(centered.width, 50);
        assert_eq!(centered.height, 10);
        assert_eq!(centered.x, 25);
        assert_eq!(centered.y, 20);
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
