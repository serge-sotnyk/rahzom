//! Sync progress and completion UI rendering

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{SyncCompleteState, SyncingState};
use crate::sync::differ::SyncAction;
use crate::ui::{format_bytes, format_duration};

/// Render the syncing progress screen
pub fn render_syncing(frame: &mut Frame, area: Rect, syncing: &SyncingState) {
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

/// Render the sync complete screen
pub fn render_sync_complete(frame: &mut Frame, area: Rect, complete: &SyncCompleteState) {
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
                    | SyncAction::DeleteLeft { path }
                    | SyncAction::CreateDirRight { path }
                    | SyncAction::CreateDirLeft { path }
                    | SyncAction::Conflict { path, .. }
                    | SyncAction::Skip { path, .. } => path.display().to_string(),
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
