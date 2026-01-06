use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::config::project::{Project, ProjectManager};

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
    Error(String),
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

    // Current project (when in ProjectView)
    pub current_project: Option<Project>,

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

                // Adjust selection
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
            Dialog::Error(_) => self.handle_key_error(code),
        }
    }

    fn handle_key_normal(&mut self, code: KeyCode) {
        match self.screen {
            Screen::ProjectList => match code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.should_quit = true;
                }
                KeyCode::Esc => {
                    self.should_quit = true;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.select_previous();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.select_next();
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
            },
            Screen::ProjectView => match code {
                KeyCode::Esc | KeyCode::Backspace => {
                    self.screen = Screen::ProjectList;
                    self.current_project = None;
                }
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.should_quit = true;
                }
                _ => {}
            },
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

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let now = Instant::now();
                let pos = (mouse.column, mouse.row);

                // Check for double-click
                if let Some((last_col, last_row, last_time)) = self.last_click {
                    if last_col == pos.0
                        && last_row == pos.1
                        && now.duration_since(last_time) < Duration::from_millis(500)
                    {
                        // Double click - open project
                        self.open_selected_project();
                        self.last_click = None;
                        return;
                    }
                }

                // Single click - select item
                if let Some(content_area) = self.content_area {
                    if mouse.column >= content_area.x
                        && mouse.column < content_area.x + content_area.width
                        && mouse.row >= content_area.y
                        && mouse.row < content_area.y + content_area.height
                    {
                        // Calculate which item was clicked (accounting for border and padding)
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
                self.select_previous();
            }
            MouseEventKind::ScrollDown => {
                self.select_next();
            }
            _ => {}
        }
    }

    fn select_next(&mut self) {
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

    fn select_previous(&mut self) {
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

    fn try_create_project(&mut self) {
        if let Dialog::NewProject(ref dialog) = self.dialog {
            // Validate
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
                        // Select the newly created project
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

        // Layout: header, content, footer
        let chunks = Layout::vertical([
            Constraint::Length(3), // Header
            Constraint::Min(1),    // Content
            Constraint::Length(3), // Footer
        ])
        .split(area);

        self.render_header(frame, chunks[0]);
        self.render_content(frame, chunks[1]);
        self.render_footer(frame, chunks[2]);

        // Render dialog on top if active
        match &self.dialog {
            Dialog::None => {}
            Dialog::NewProject(dialog) => {
                self.render_new_project_dialog(frame, dialog.clone());
            }
            Dialog::DeleteConfirm(name) => {
                self.render_delete_confirm_dialog(frame, name);
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
            Screen::ProjectList => "Projects",
            Screen::ProjectView => {
                if let Some(ref p) = self.current_project {
                    &p.name
                } else {
                    "Project"
                }
            }
            Screen::Analyzing => "Analyzing",
            Screen::Preview => "Preview",
            Screen::Syncing => "Syncing",
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
            .map(|name| {
                ListItem::new(Line::from(format!("  {}  ", name)))
            })
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
                Line::from(Span::styled(
                    "(Analyze & Preview will be implemented in Stage 8)",
                    Style::default().fg(Color::DarkGray),
                )),
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
                        Span::raw(" Navigate  "),
                        Span::styled(" Enter ", Style::default().fg(Color::Black).bg(Color::Gray)),
                        Span::raw(" Open  "),
                        Span::styled(" N ", Style::default().fg(Color::Black).bg(Color::Gray)),
                        Span::raw(" New  "),
                        Span::styled(" D ", Style::default().fg(Color::Black).bg(Color::Gray)),
                        Span::raw(" Delete  "),
                        Span::styled(" Q ", Style::default().fg(Color::Black).bg(Color::Gray)),
                        Span::raw(" Quit "),
                    ]
                }
            }
            Screen::ProjectView => {
                vec![
                    Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Back  "),
                    Span::styled(" Q ", Style::default().fg(Color::Black).bg(Color::Gray)),
                    Span::raw(" Quit "),
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

        // Clear the area behind dialog
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" New Project ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::vertical([
            Constraint::Length(1), // Padding
            Constraint::Length(2), // Name field
            Constraint::Length(1), // Padding
            Constraint::Length(2), // Left path field
            Constraint::Length(1), // Padding
            Constraint::Length(2), // Right path field
            Constraint::Length(1), // Padding
            Constraint::Min(1),    // Error / hints
        ])
        .split(inner.inner(Margin::new(2, 0)));

        // Name field
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

        // Left path field
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

        // Right path field
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

        // Error or hints
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

/// Helper function to create a centered rect
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
    fn test_new_project_dialog_text_input() {
        let (mut app, _temp) = create_test_app();
        app.dialog = Dialog::NewProject(NewProjectDialog::new());

        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Char('e'));
        app.handle_key(KeyCode::Char('s'));
        app.handle_key(KeyCode::Char('t'));

        if let Dialog::NewProject(ref d) = app.dialog {
            assert_eq!(d.name, "test");
        }
    }

    #[test]
    fn test_new_project_dialog_backspace() {
        let (mut app, _temp) = create_test_app();
        app.dialog = Dialog::NewProject(NewProjectDialog::new());

        app.handle_key(KeyCode::Char('a'));
        app.handle_key(KeyCode::Char('b'));
        app.handle_key(KeyCode::Backspace);

        if let Dialog::NewProject(ref d) = app.dialog {
            assert_eq!(d.name, "a");
        }
    }

    #[test]
    fn test_create_project() {
        let (mut app, _temp) = create_test_app();

        // Open dialog and fill in fields
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
    fn test_create_project_validation_empty_name() {
        let (mut app, _temp) = create_test_app();

        app.dialog = Dialog::NewProject(NewProjectDialog {
            name: "".to_string(),
            left_path: "/path/left".to_string(),
            right_path: "/path/right".to_string(),
            focused_field: DialogField::Name,
            error: None,
        });

        app.try_create_project();

        if let Dialog::NewProject(ref d) = app.dialog {
            assert!(d.error.is_some());
        } else {
            panic!("Dialog should still be open");
        }
    }

    #[test]
    fn test_select_next_wraps() {
        let (mut app, _temp) = create_test_app();
        app.projects = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        app.list_state.select(Some(2));

        app.select_next();

        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_select_previous_wraps() {
        let (mut app, _temp) = create_test_app();
        app.projects = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        app.list_state.select(Some(0));

        app.select_previous();

        assert_eq!(app.list_state.selected(), Some(2));
    }

    #[test]
    fn test_open_project() {
        let (mut app, _temp) = create_test_app();

        // Create a project first
        let pm = app.project_manager.as_ref().unwrap();
        let project = Project::new("test", PathBuf::from("/left"), PathBuf::from("/right"));
        pm.save_project(&project).unwrap();
        app.refresh_projects();

        // Open it
        app.list_state.select(Some(0));
        app.open_selected_project();

        assert_eq!(app.screen, Screen::ProjectView);
        assert!(app.current_project.is_some());
        assert_eq!(app.current_project.as_ref().unwrap().name, "test");
    }

    #[test]
    fn test_delete_project() {
        let (mut app, _temp) = create_test_app();

        // Create a project
        let pm = app.project_manager.as_ref().unwrap();
        let project = Project::new("to-delete", PathBuf::from("/left"), PathBuf::from("/right"));
        pm.save_project(&project).unwrap();
        app.refresh_projects();

        assert_eq!(app.projects.len(), 1);

        // Delete it
        app.delete_project("to-delete");

        assert!(app.projects.is_empty());
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
    fn test_delete_confirm_dialog() {
        let (mut app, _temp) = create_test_app();

        // Create a project
        let pm = app.project_manager.as_ref().unwrap();
        let project = Project::new("test", PathBuf::from("/l"), PathBuf::from("/r"));
        pm.save_project(&project).unwrap();
        app.refresh_projects();
        app.list_state.select(Some(0));

        // Press D to open delete confirm
        app.handle_key(KeyCode::Char('d'));
        assert!(matches!(app.dialog, Dialog::DeleteConfirm(_)));

        // Press N to cancel
        app.handle_key(KeyCode::Char('n'));
        assert!(matches!(app.dialog, Dialog::None));
        assert_eq!(app.projects.len(), 1);
    }

    #[test]
    fn test_delete_confirm_yes() {
        let (mut app, _temp) = create_test_app();

        // Create a project
        let pm = app.project_manager.as_ref().unwrap();
        let project = Project::new("test", PathBuf::from("/l"), PathBuf::from("/r"));
        pm.save_project(&project).unwrap();
        app.refresh_projects();
        app.list_state.select(Some(0));

        // Press D then Y
        app.handle_key(KeyCode::Char('d'));
        app.handle_key(KeyCode::Char('y'));

        assert!(matches!(app.dialog, Dialog::None));
        assert!(app.projects.is_empty());
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
}
