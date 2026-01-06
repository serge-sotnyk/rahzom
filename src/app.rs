use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseEventKind};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::time::Duration;

/// Application screens
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    ProjectList,
    ProjectView,
    Analyzing,
    Preview,
    Syncing,
}

/// Main application state
pub struct App {
    pub screen: Screen,
    pub should_quit: bool,
    pub last_mouse_event: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::ProjectList,
            should_quit: false,
            last_mouse_event: None,
        }
    }

    /// Main application loop
    pub fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> anyhow::Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.render(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    /// Handle input events
    fn handle_events(&mut self) -> anyhow::Result<()> {
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    self.handle_key(key.code);
                }
                Event::Mouse(mouse) => {
                    self.handle_mouse(mouse);
                }
                Event::Resize(_, _) => {
                    // Terminal will redraw automatically
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Handle keyboard input
    fn handle_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.should_quit = true;
            }
            KeyCode::Esc => {
                self.should_quit = true;
            }
            _ => {}
        }
    }

    /// Handle mouse input
    fn handle_mouse(&mut self, mouse: event::MouseEvent) {
        let event_type = match mouse.kind {
            MouseEventKind::Down(btn) => format!("Click {:?}", btn),
            MouseEventKind::Up(btn) => format!("Release {:?}", btn),
            MouseEventKind::Drag(btn) => format!("Drag {:?}", btn),
            MouseEventKind::Moved => "Move".to_string(),
            MouseEventKind::ScrollDown => "ScrollDown".to_string(),
            MouseEventKind::ScrollUp => "ScrollUp".to_string(),
            MouseEventKind::ScrollLeft => "ScrollLeft".to_string(),
            MouseEventKind::ScrollRight => "ScrollRight".to_string(),
        };
        self.last_mouse_event = Some(format!("{} at ({}, {})", event_type, mouse.column, mouse.row));
    }

    /// Render the application
    fn render(&self, frame: &mut Frame) {
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
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let version = env!("CARGO_PKG_VERSION");
        let title = format!(" rahzom v{} ", version);

        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                title,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("— folder synchronization"),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(header, area);
    }

    fn render_content(&self, frame: &mut Frame, area: Rect) {
        let screen_name = match self.screen {
            Screen::ProjectList => "Project List",
            Screen::ProjectView => "Project View",
            Screen::Analyzing => "Analyzing...",
            Screen::Preview => "Preview",
            Screen::Syncing => "Syncing...",
        };

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("Current screen: {}", screen_name),
                Style::default().fg(Color::Yellow),
            )),
            Line::from(""),
        ];

        if let Some(ref mouse_event) = self.last_mouse_event {
            lines.push(Line::from(Span::styled(
                format!("Last mouse event: {}", mouse_event),
                Style::default().fg(Color::Green),
            )));
        }

        let content = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Content ")
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(content, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hints = Line::from(vec![
            Span::styled(" Q ", Style::default().fg(Color::Black).bg(Color::Gray)),
            Span::raw(" Quit  "),
            Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
            Span::raw(" Exit "),
        ]);

        let footer = Paragraph::new(hints).block(
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

    #[test]
    fn test_app_initial_state() {
        let app = App::new();
        assert_eq!(app.screen, Screen::ProjectList);
        assert!(!app.should_quit);
        assert!(app.last_mouse_event.is_none());
    }

    #[test]
    fn test_quit_on_q() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn test_quit_on_esc() {
        let mut app = App::new();
        app.handle_key(KeyCode::Esc);
        assert!(app.should_quit);
    }

    #[test]
    fn test_mouse_event_tracking() {
        let mut app = App::new();
        let mouse = event::MouseEvent {
            kind: MouseEventKind::Down(event::MouseButton::Left),
            column: 10,
            row: 5,
            modifiers: event::KeyModifiers::empty(),
        };
        app.handle_mouse(mouse);
        assert!(app.last_mouse_event.is_some());
        assert!(app.last_mouse_event.as_ref().unwrap().contains("10"));
        assert!(app.last_mouse_event.as_ref().unwrap().contains("5"));
    }

    #[test]
    fn test_default_impl() {
        let app = App::default();
        assert_eq!(app.screen, Screen::ProjectList);
    }
}
