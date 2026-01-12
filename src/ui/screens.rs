//! Screen rendering functions

use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};

use crate::app::{PreviewState, PreviewSummary, UserAction};
use crate::config::project::Project;
use crate::sync::differ::{ConflictReason, SyncAction};
use crate::ui::format_bytes;

/// Render the project list screen
pub fn render_project_list(
    frame: &mut Frame,
    area: Rect,
    projects: &[String],
    list_state: &mut ListState,
) {
    if projects.is_empty() {
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

    let items: Vec<ListItem> = projects
        .iter()
        .map(|name| ListItem::new(Line::from(format!("  {}  ", name))))
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Projects ({}) ", projects.len()))
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, list_state);
}

/// Render the project view screen
pub fn render_project_view(frame: &mut Frame, area: Rect, project: Option<&Project>) {
    let content = if let Some(project) = project {
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

/// Render the preview screen with action list and summary
pub fn render_preview(frame: &mut Frame, area: Rect, preview: &PreviewState) {
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
    render_summary(frame, chunks[1], &summary);
}

/// Render the preview summary
pub fn render_summary(frame: &mut Frame, area: Rect, summary: &PreviewSummary) {
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

/// Render a single action item in the preview list
pub fn render_action_item(
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
        UserAction::DeleteLeft { path } => ("←✕*", Color::Red, path.display().to_string()),
        UserAction::DeleteRight { path } => ("✕→*", Color::Red, path.display().to_string()),
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
