//! Screen rendering functions

use ratatui::{
    layout::{Alignment, Constraint, Layout, Margin, Rect},
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
use crate::ui::{format_bytes, truncate_middle};

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

/// Minimum readable width per side for the one-line L/R legend.
const LEGEND_MIN_PER_SIDE: usize = 16;

/// Render the preview screen with L/R legend, action list, status bar, summary.
pub fn render_preview(
    frame: &mut Frame,
    area: Rect,
    preview: &PreviewState,
    project: Option<&Project>,
) {
    // Decide whether the L/R legend fits on a single line.
    let inner_cols = area.width.saturating_sub(2) as usize;
    let two_line_legend = inner_cols < LEGEND_MIN_PER_SIDE * 2 + 4;
    let legend_height = if two_line_legend { 2 } else { 1 };

    let chunks = Layout::vertical([
        Constraint::Min(0),    // Actions block (legend + list)
        Constraint::Length(4), // Summary
    ])
    .split(area);

    // The actions panel hosts both the legend and the list, but we want the
    // list to occupy the remaining space. We split the actions block area
    // again after rendering its outer block.
    let indices = preview.sorted_filtered_indices();

    let actions_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " Actions ({}/{}) [Sort: {}] ",
            indices.len(),
            preview.actions.len(),
            preview.sort.label()
        ))
        .border_style(Style::default().fg(Color::DarkGray));
    let actions_inner = actions_block.inner(chunks[0]);
    frame.render_widget(actions_block, chunks[0]);

    // The legend lives at the top of the actions inner area; list below it.
    let inner_chunks =
        Layout::vertical([Constraint::Length(legend_height as u16), Constraint::Min(0)])
            .split(actions_inner);

    render_legend(frame, inner_chunks[0], project, two_line_legend);

    // Outer block already drew the title/border; here we render the rows
    // directly without an additional inner block.
    let list_area = inner_chunks[1];
    let visible_height = list_area.height as usize;
    let scroll_offset = if preview.selected >= visible_height && visible_height > 0 {
        preview.selected - visible_height + 1
    } else {
        0
    };

    if indices.is_empty() {
        let hint = Paragraph::new(Line::from(Span::styled(
            "(no items match filter — press F to switch)",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hint, list_area);
    } else {
        let row_width = list_area.width as usize;
        let items: Vec<ListItem> = indices
            .iter()
            .skip(scroll_offset)
            .take(visible_height)
            .enumerate()
            .map(|(display_idx, &real_idx)| {
                let action = &preview.actions[real_idx];
                let is_selected = display_idx + scroll_offset == preview.selected;
                let is_marked = preview.selected_items.contains(&real_idx);
                render_action_item(action, is_selected, is_marked, row_width)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, list_area);

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
    }

    // Summary
    let summary = preview.summary();
    render_summary(frame, chunks[1], &summary);
}

/// Render the L/R folder legend. On wide terminals one line shows left path
/// left-aligned and right path right-aligned; on narrow ones they stack.
fn render_legend(frame: &mut Frame, area: Rect, project: Option<&Project>, two_line: bool) {
    let (left, right) = match project {
        Some(p) => (
            p.left_path.display().to_string(),
            p.right_path.display().to_string(),
        ),
        None => (String::new(), String::new()),
    };

    let cols = area.width as usize;
    let style = Style::default().fg(Color::Cyan);

    if two_line {
        let left_line = Paragraph::new(Line::from(Span::styled(
            truncate_middle(&left, cols).into_owned(),
            style,
        )))
        .alignment(Alignment::Left);
        let right_line = Paragraph::new(Line::from(Span::styled(
            truncate_middle(&right, cols).into_owned(),
            style,
        )))
        .alignment(Alignment::Right);

        let split = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);
        frame.render_widget(left_line, split[0]);
        frame.render_widget(right_line, split[1]);
    } else {
        // Reserve a 1-char gap between the two halves.
        let half = cols.saturating_sub(1) / 2;
        let split = Layout::horizontal([
            Constraint::Length(half as u16),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

        let left_par = Paragraph::new(Line::from(Span::styled(
            truncate_middle(&left, half).into_owned(),
            style,
        )))
        .alignment(Alignment::Left);
        let right_par = Paragraph::new(Line::from(Span::styled(
            truncate_middle(&right, split[2].width as usize).into_owned(),
            style,
        )))
        .alignment(Alignment::Right);

        frame.render_widget(left_par, split[0]);
        frame.render_widget(right_par, split[2]);
    }
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

/// Render a single action item in the preview list. The full row width is
/// passed in so the path can be middle-truncated to fit; trailing tags
/// (size, conflict reason) are reserved first so the user keeps that context.
pub fn render_action_item(
    action: &UserAction,
    is_selected: bool,
    is_marked: bool,
    row_width: usize,
) -> ListItem<'static> {
    let (symbol, color, path_str, tag) = decompose_action(action);

    let marker = if is_marked { "● " } else { "  " };
    let modified_indicator = if action.is_modified() { "*" } else { "" };

    // Prefix: marker (2) + symbol column (3) + space (1).
    let prefix_chars = marker.chars().count() + 3 + 1;
    let modified_chars = modified_indicator.chars().count();
    let tag_chars = tag.as_ref().map(|t| t.chars().count()).unwrap_or(0);

    let available = row_width
        .saturating_sub(prefix_chars)
        .saturating_sub(tag_chars)
        .saturating_sub(modified_chars);
    let truncated_path = truncate_middle(&path_str, available).into_owned();

    let style = if is_selected {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default()
    };

    let mut spans = vec![
        Span::raw(marker),
        Span::styled(format!("{:<3}", symbol), Style::default().fg(color)),
        Span::raw(" "),
        Span::styled(truncated_path, style),
    ];
    if let Some(tag_str) = tag {
        spans.push(Span::styled(tag_str, Style::default().fg(Color::DarkGray)));
    }
    spans.push(Span::styled(
        modified_indicator,
        Style::default().fg(Color::Magenta),
    ));

    ListItem::new(Line::from(spans))
}

/// Returns (symbol, color, path_string, optional_trailing_tag_with_leading_space).
fn decompose_action(action: &UserAction) -> (&'static str, Color, String, Option<String>) {
    match action {
        UserAction::Original(SyncAction::CopyToRight { path, size }) => (
            "→",
            Color::Green,
            path.display().to_string(),
            Some(format!(" ({})", format_bytes(*size))),
        ),
        UserAction::Original(SyncAction::CopyToLeft { path, size }) => (
            "←",
            Color::Blue,
            path.display().to_string(),
            Some(format!(" ({})", format_bytes(*size))),
        ),
        UserAction::Original(SyncAction::DeleteRight { path }) => {
            ("✕→", Color::Red, path.display().to_string(), None)
        }
        UserAction::Original(SyncAction::DeleteLeft { path }) => {
            ("←✕", Color::Red, path.display().to_string(), None)
        }
        UserAction::Original(SyncAction::CreateDirRight { path }) => {
            ("📁→", Color::Green, path.display().to_string(), None)
        }
        UserAction::Original(SyncAction::CreateDirLeft { path }) => {
            ("←📁", Color::Blue, path.display().to_string(), None)
        }
        UserAction::Original(SyncAction::Conflict { path, reason, .. }) => {
            let reason_str = conflict_reason_label(reason);
            (
                "⚠",
                Color::Yellow,
                path.display().to_string(),
                Some(format!(" ({})", reason_str)),
            )
        }
        UserAction::Original(SyncAction::Skip { path, .. }) => {
            ("·", Color::DarkGray, path.display().to_string(), None)
        }
        UserAction::CopyToRight { path, size } => (
            "→*",
            Color::Green,
            path.display().to_string(),
            Some(format!(" ({})", format_bytes(*size))),
        ),
        UserAction::CopyToLeft { path, size } => (
            "←*",
            Color::Blue,
            path.display().to_string(),
            Some(format!(" ({})", format_bytes(*size))),
        ),
        UserAction::DeleteLeft { path } => ("←✕*", Color::Red, path.display().to_string(), None),
        UserAction::DeleteRight { path } => ("✕→*", Color::Red, path.display().to_string(), None),
        UserAction::Skip { path } => ("·*", Color::DarkGray, path.display().to_string(), None),
    }
}

pub(crate) fn conflict_reason_label(reason: &ConflictReason) -> &'static str {
    match reason {
        ConflictReason::BothModified => "both modified",
        ConflictReason::ModifiedAndDeleted => "mod vs del",
        ConflictReason::ExistsVsDeleted => "exists vs del",
        ConflictReason::CaseConflict => "case conflict",
    }
}
