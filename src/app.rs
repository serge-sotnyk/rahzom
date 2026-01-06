use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::project::{Project, ProjectManager};
use crate::sync::differ::{diff, ConflictReason, DiffResult, SyncAction};
use crate::sync::metadata::SyncMetadata;
use crate::sync::scanner::{scan, ScanResult};

/// Application screens
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    ProjectList,
    ProjectView,
    Analyzing,
    Preview,
    Syncing,
}

/// Dialog mode for project list screen
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    None,
    NewProject(NewProjectDialog),
    DeleteConfirm(String),
    CreateDirConfirm { path: PathBuf, is_left: bool },
    Error(String),
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
            self.handle_events()?;
        }
        Ok(())
    }

    /// Handle input events
    fn handle_events(&mut self) -> Result<()> {
        if event::poll(Duration::from_millis(100))? {
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
        }
    }

    fn handle_key_normal(&mut self, code: KeyCode) {
        match self.screen {
            Screen::ProjectList => self.handle_key_project_list(code),
            Screen::ProjectView => self.handle_key_project_view(code),
            Screen::Preview => self.handle_key_preview(code),
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
                    let size = get_action_size(action);
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
                    let size = get_action_size(action);
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
            let mut scrollbar_state =
                ScrollbarState::new(indices.len()).position(preview.selected);
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
                    Span::styled(" F ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Filter  "),
                    Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Back "),
                ]
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

fn get_action_size(action: &UserAction) -> u64 {
    match action {
        UserAction::Original(SyncAction::CopyToRight { size, .. }) => *size,
        UserAction::Original(SyncAction::CopyToLeft { size, .. }) => *size,
        UserAction::CopyToRight { size, .. } => *size,
        UserAction::CopyToLeft { size, .. } => *size,
        _ => 0,
    }
}

fn render_action_item(action: &UserAction, is_selected: bool, is_marked: bool) -> ListItem<'static> {
    let (symbol, color, path_str) = match action {
        UserAction::Original(SyncAction::CopyToRight { path, size }) => {
            ("→", Color::Green, format!("{} ({})", path.display(), format_bytes(*size)))
        }
        UserAction::Original(SyncAction::CopyToLeft { path, size }) => {
            ("←", Color::Blue, format!("{} ({})", path.display(), format_bytes(*size)))
        }
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
            ("⚠", Color::Yellow, format!("{} ({})", path.display(), reason_str))
        }
        UserAction::Original(SyncAction::Skip { path, .. }) => {
            ("·", Color::DarkGray, path.display().to_string())
        }
        UserAction::CopyToRight { path, size } => {
            ("→*", Color::Green, format!("{} ({})", path.display(), format_bytes(*size)))
        }
        UserAction::CopyToLeft { path, size } => {
            ("←*", Color::Blue, format!("{} ({})", path.display(), format_bytes(*size)))
        }
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
        app.current_project = Some(Project::new("test", PathBuf::from("/l"), PathBuf::from("/r")));

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
