//! Dialog rendering functions

use std::path::Path;

use ratatui::layout::{Alignment, Constraint, Layout, Margin};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{DialogField, ExclusionsInfoDialog, NewProjectDialog, SyncConfirmDialog};
use crate::ui::{centered_rect, format_bytes};

/// Renders new project dialog
pub fn render_new_project_dialog(frame: &mut Frame, dialog: &NewProjectDialog) {
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

/// Renders delete confirmation dialog
pub fn render_delete_confirm_dialog(frame: &mut Frame, name: &str) {
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

    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

/// Renders create directory confirmation dialog
pub fn render_create_dir_confirm_dialog(frame: &mut Frame, path: &Path, is_left: bool) {
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

    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

/// Renders error dialog
pub fn render_error_dialog(frame: &mut Frame, message: &str) {
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

    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

/// Renders sync confirmation dialog
pub fn render_sync_confirm_dialog(frame: &mut Frame, dialog: &SyncConfirmDialog) {
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

    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

/// Renders cancel sync confirmation dialog
pub fn render_cancel_sync_confirm_dialog(frame: &mut Frame) {
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

    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

/// Renders exclusions info dialog
pub fn render_exclusions_info_dialog(frame: &mut Frame, dialog: &ExclusionsInfoDialog) {
    let area = centered_rect(70, 14, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Exclusion Patterns (.rahzomignore) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let left_status = if dialog.left_exists {
        Span::styled(
            format!("{} patterns", dialog.left_count),
            Style::default().fg(Color::Green),
        )
    } else {
        Span::styled("not created", Style::default().fg(Color::DarkGray))
    };

    let right_status = if dialog.right_exists {
        Span::styled(
            format!("{} patterns", dialog.right_count),
            Style::default().fg(Color::Green),
        )
    } else {
        Span::styled("not created", Style::default().fg(Color::DarkGray))
    };

    let can_create = !dialog.left_exists || !dialog.right_exists;

    let mut text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Left:  ", Style::default().fg(Color::DarkGray)),
            left_status,
        ]),
        Line::from(Span::styled(
            format!("  {}", dialog.left_path.display()),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Right: ", Style::default().fg(Color::DarkGray)),
            right_status,
        ]),
        Line::from(Span::styled(
            format!("  {}", dialog.right_path.display()),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
    ];

    if can_create {
        text.push(Line::from(vec![
            Span::styled(" T ", Style::default().fg(Color::Black).bg(Color::Green)),
            Span::raw(" Create template  "),
            Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
            Span::raw(" Close"),
        ]));
    } else {
        text.push(Line::from(Span::styled(
            "Edit .rahzomignore files manually",
            Style::default().fg(Color::DarkGray),
        )));
        text.push(Line::from(vec![
            Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
            Span::raw(" Close"),
        ]));
    }

    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}
