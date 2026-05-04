//! Dialog rendering functions

use std::path::Path;

use ratatui::layout::{Alignment, Constraint, Layout, Margin};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{
    DialogField, DiskSpaceWarningDialog, ExclusionsInfoDialog, FileErrorDialog, NewProjectDialog,
    PreviewState, SettingsDialog, SettingsField, SyncConfirmDialog, UserAction,
};
use crate::config::project::Project;
use crate::sync::differ::SyncAction;
use crate::sync::executor::SyncErrorKind;
use crate::sync::scanner::{FileEntry, ScanResult};
use crate::ui::screens::conflict_reason_label;
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

/// Renders disk space warning dialog
pub fn render_disk_space_warning_dialog(frame: &mut Frame, dialog: &DiskSpaceWarningDialog) {
    let area = centered_rect(60, 11, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Low Disk Space ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let side = if dialog.is_left { "Left" } else { "Right" };
    let text = vec![
        Line::from(""),
        Line::from(format!("{} destination may not have", side)),
        Line::from("enough space:"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Required:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(dialog.required),
                Style::default().fg(Color::Red),
            ),
        ]),
        Line::from(vec![
            Span::styled("Available: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(dialog.available),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Y ", Style::default().fg(Color::Black).bg(Color::Yellow)),
            Span::raw(" Continue anyway  "),
            Span::styled(" N ", Style::default().fg(Color::Black).bg(Color::Gray)),
            Span::raw(" Cancel"),
        ]),
    ];

    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

/// Renders file error dialog (locked file, permission denied)
pub fn render_file_error_dialog(frame: &mut Frame, dialog: &FileErrorDialog) {
    let area = centered_rect(65, 11, frame.area());
    frame.render_widget(Clear, area);

    let (title, title_color) = match dialog.kind {
        SyncErrorKind::FileLocked => (" File Locked ", Color::Yellow),
        SyncErrorKind::PermissionDenied => (" Permission Denied ", Color::Red),
        _ => (" Error ", Color::Red),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(title_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let path_str = dialog.path.display().to_string();
    let show_retry = matches!(dialog.kind, SyncErrorKind::FileLocked);

    let mut text = vec![
        Line::from(""),
        Line::from("Cannot access file:"),
        Line::from(Span::styled(
            if path_str.len() > 55 {
                format!("...{}", &path_str[path_str.len() - 52..])
            } else {
                path_str
            },
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(Span::styled(&dialog.error, Style::default().fg(Color::Red))),
        Line::from(""),
    ];

    if show_retry {
        text.push(Line::from(vec![
            Span::styled(" R ", Style::default().fg(Color::Black).bg(Color::Yellow)),
            Span::raw(" Retry  "),
            Span::styled(" S ", Style::default().fg(Color::Black).bg(Color::Gray)),
            Span::raw(" Skip  "),
            Span::styled(" C ", Style::default().fg(Color::Black).bg(Color::Red)),
            Span::raw(" Cancel"),
        ]));
    } else {
        text.push(Line::from(vec![
            Span::styled(" S ", Style::default().fg(Color::Black).bg(Color::Gray)),
            Span::raw(" Skip  "),
            Span::styled(" C ", Style::default().fg(Color::Black).bg(Color::Red)),
            Span::raw(" Cancel"),
        ]));
    }

    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

/// Renders project settings dialog
pub fn render_settings_dialog(frame: &mut Frame, dialog: &SettingsDialog) {
    let area = centered_rect(55, 14, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Project Settings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(1), // spacing
        Constraint::Length(1), // backup versions
        Constraint::Length(1), // spacing
        Constraint::Length(1), // retention days
        Constraint::Length(1), // spacing
        Constraint::Length(1), // soft delete
        Constraint::Length(1), // spacing
        Constraint::Length(1), // verify hash
        Constraint::Length(1), // spacing
        Constraint::Min(1),    // hints/error
    ])
    .split(inner.inner(Margin::new(2, 0)));

    // Backup versions field
    let backup_style = if dialog.focused_field == SettingsField::BackupVersions {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let backup_line = Line::from(vec![
        Span::styled("Backup versions:    ", backup_style),
        Span::raw(&dialog.backup_versions),
        if dialog.focused_field == SettingsField::BackupVersions {
            Span::styled("▌", Style::default().fg(Color::White))
        } else {
            Span::raw("")
        },
        Span::styled(" (1-100)", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(backup_line), chunks[1]);

    // Retention days field
    let retention_style = if dialog.focused_field == SettingsField::DeletedRetentionDays {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let retention_line = Line::from(vec![
        Span::styled("Deleted retention:  ", retention_style),
        Span::raw(&dialog.deleted_retention_days),
        if dialog.focused_field == SettingsField::DeletedRetentionDays {
            Span::styled("▌", Style::default().fg(Color::White))
        } else {
            Span::raw("")
        },
        Span::styled(" days (0=off)", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(retention_line), chunks[3]);

    // Soft delete toggle
    let soft_style = if dialog.focused_field == SettingsField::SoftDelete {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let soft_value = if dialog.soft_delete { "Yes" } else { "No " };
    let soft_line = Line::from(vec![
        Span::styled("Soft delete:        ", soft_style),
        Span::styled(
            format!("[{}]", soft_value),
            if dialog.soft_delete {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            },
        ),
        Span::styled(" (Space to toggle)", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(soft_line), chunks[5]);

    // Verify hash toggle
    let hash_style = if dialog.focused_field == SettingsField::VerifyHash {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let hash_value = if dialog.verify_hash { "Yes" } else { "No " };
    let hash_line = Line::from(vec![
        Span::styled("Verify hash:        ", hash_style),
        Span::styled(
            format!("[{}]", hash_value),
            if dialog.verify_hash {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            },
        ),
        Span::styled(" (Space to toggle)", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(hash_line), chunks[7]);

    // Hints or error
    let hint = if let Some(ref error) = dialog.error {
        Line::from(Span::styled(error, Style::default().fg(Color::Red)))
    } else {
        Line::from(vec![
            Span::styled(" Tab ", Style::default().fg(Color::Black).bg(Color::Gray)),
            Span::raw(" Next  "),
            Span::styled(
                " Enter ",
                Style::default().fg(Color::Black).bg(Color::Green),
            ),
            Span::raw(" Save  "),
            Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
            Span::raw(" Cancel"),
        ])
    };
    frame.render_widget(Paragraph::new(hint), chunks[9]);
}

/// Renders the per-action details popup shown on Enter from the preview list.
pub fn render_action_details_dialog(
    frame: &mut Frame,
    preview: &PreviewState,
    project: Option<&Project>,
    action_index: usize,
) {
    let Some(action) = preview.actions.get(action_index) else {
        return;
    };

    let frame_area = frame.area();
    // ~70% width, generous min height so wrapped paths and metadata fit.
    let area = centered_rect(70, 14, frame_area);
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Action details ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(1), // spacing
        Constraint::Min(1),    // body
        Constraint::Length(1), // footer
    ])
    .split(inner.inner(Margin::new(1, 0)));

    let mut lines: Vec<Line> = Vec::new();

    // 1. Full path (wrapped via Paragraph::wrap below).
    lines.push(Line::from(vec![
        Span::styled("Path: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            action.path().display().to_string(),
            Style::default().fg(Color::Cyan),
        ),
    ]));
    lines.push(Line::from(""));

    // 2. Action and direction.
    let (icon, color, description) = describe_action(action, project);
    let mut action_spans = vec![
        Span::styled("Action: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{} ", icon), Style::default().fg(color)),
        Span::raw(description),
    ];
    if action.is_modified() {
        action_spans.push(Span::raw("  "));
        action_spans.push(Span::styled(
            "(modified by user)",
            Style::default().fg(Color::Magenta),
        ));
    }
    lines.push(Line::from(action_spans));

    // 3. Optional reason for conflicts/skips.
    if let Some(reason) = describe_reason(action) {
        lines.push(Line::from(vec![
            Span::styled("Reason: ", Style::default().fg(Color::DarkGray)),
            Span::styled(reason, Style::default().fg(Color::Yellow)),
        ]));
    }
    lines.push(Line::from(""));

    // 4. Left/right metadata.
    let left_entry = lookup_entry(preview.left_scan.as_ref(), action.path());
    let right_entry = lookup_entry(preview.right_scan.as_ref(), action.path());
    lines.push(metadata_line("Left:  ", left_entry));
    lines.push(metadata_line("Right: ", right_entry));

    let body = Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(body, chunks[1]);

    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
        Span::raw(" / "),
        Span::styled(" Enter ", Style::default().fg(Color::Black).bg(Color::Gray)),
        Span::raw(" Close"),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(footer, chunks[2]);
}

fn describe_action(
    action: &UserAction,
    project: Option<&Project>,
) -> (&'static str, Color, String) {
    let (left_path, right_path) = match project {
        Some(p) => (
            p.left_path.display().to_string(),
            p.right_path.display().to_string(),
        ),
        None => ("LEFT".to_string(), "RIGHT".to_string()),
    };

    match action {
        UserAction::Original(SyncAction::CopyToRight { size, .. }) => (
            "→",
            Color::Green,
            format!(
                "Copy from {} to {} ({})",
                left_path,
                right_path,
                format_bytes(*size)
            ),
        ),
        UserAction::Original(SyncAction::CopyToLeft { size, .. }) => (
            "←",
            Color::Blue,
            format!(
                "Copy from {} to {} ({})",
                right_path,
                left_path,
                format_bytes(*size)
            ),
        ),
        UserAction::Original(SyncAction::DeleteRight { .. }) => {
            ("✕→", Color::Red, format!("Delete from {}", right_path))
        }
        UserAction::Original(SyncAction::DeleteLeft { .. }) => {
            ("←✕", Color::Red, format!("Delete from {}", left_path))
        }
        UserAction::Original(SyncAction::CreateDirRight { .. }) => (
            "📁→",
            Color::Green,
            format!("Create directory in {}", right_path),
        ),
        UserAction::Original(SyncAction::CreateDirLeft { .. }) => (
            "←📁",
            Color::Blue,
            format!("Create directory in {}", left_path),
        ),
        UserAction::Original(SyncAction::Conflict { .. }) => (
            "⚠",
            Color::Yellow,
            "Conflict — needs resolution".to_string(),
        ),
        UserAction::Original(SyncAction::Skip { .. }) | UserAction::Skip { .. } => {
            ("·", Color::DarkGray, "Skip — no action".to_string())
        }
        UserAction::CopyToRight { size, .. } => (
            "→",
            Color::Green,
            format!(
                "Copy from {} to {} ({})",
                left_path,
                right_path,
                format_bytes(*size)
            ),
        ),
        UserAction::CopyToLeft { size, .. } => (
            "←",
            Color::Blue,
            format!(
                "Copy from {} to {} ({})",
                right_path,
                left_path,
                format_bytes(*size)
            ),
        ),
        UserAction::DeleteRight { .. } => ("✕→", Color::Red, format!("Delete from {}", right_path)),
        UserAction::DeleteLeft { .. } => ("←✕", Color::Red, format!("Delete from {}", left_path)),
    }
}

fn describe_reason(action: &UserAction) -> Option<String> {
    match action {
        UserAction::Original(SyncAction::Conflict { reason, .. }) => {
            Some(conflict_reason_label(reason).to_string())
        }
        UserAction::Original(SyncAction::Skip { reason, .. }) => Some(reason.clone()),
        _ => None,
    }
}

fn lookup_entry<'a>(scan: Option<&'a ScanResult>, target: &Path) -> Option<&'a FileEntry> {
    scan?.entries.iter().find(|e| e.path == target)
}

fn metadata_line(label: &'static str, entry: Option<&FileEntry>) -> Line<'static> {
    let mut spans = vec![Span::styled(label, Style::default().fg(Color::DarkGray))];
    match entry {
        Some(e) if e.is_dir => {
            spans.push(Span::styled(
                format!("(dir)        {}", e.mtime.format("%Y-%m-%d %H:%M")),
                Style::default(),
            ));
        }
        Some(e) => {
            spans.push(Span::styled(
                format!(
                    "{:>10}  {}",
                    format_bytes(e.size),
                    e.mtime.format("%Y-%m-%d %H:%M")
                ),
                Style::default(),
            ));
        }
        None => {
            spans.push(Span::styled("—", Style::default().fg(Color::DarkGray)));
        }
    }
    Line::from(spans)
}
