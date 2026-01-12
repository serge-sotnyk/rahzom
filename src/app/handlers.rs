//! Event handling for the application

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use std::time::{Duration, Instant};

use super::{App, Dialog, NewProjectDialog, Screen, UserAction};

impl App {
    /// Handle input events
    pub(super) fn handle_events(&mut self) -> Result<()> {
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
    pub fn handle_key(&mut self, code: KeyCode) {
        match &self.dialog {
            Dialog::None => self.handle_key_normal(code),
            Dialog::NewProject(_) => self.handle_key_new_project(code),
            Dialog::DeleteConfirm(_) => self.handle_key_delete_confirm(code),
            Dialog::CreateDirConfirm { .. } => self.handle_key_create_dir_confirm(code),
            Dialog::Error(_) => self.handle_key_error(code),
            Dialog::SyncConfirm(_) => self.handle_key_sync_confirm(code),
            Dialog::CancelSyncConfirm => self.handle_key_cancel_sync_confirm(code),
            Dialog::ExclusionsInfo(_) => self.handle_key_exclusions_info(code),
            Dialog::DiskSpaceWarning(_) => self.handle_key_disk_space_warning(code),
            Dialog::FileError(_) => self.handle_key_file_error(code),
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
            KeyCode::Char('e') | KeyCode::Char('E') => {
                self.show_exclusions_dialog();
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
                self.start_sync(false);
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

    fn handle_key_exclusions_info(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.dialog = Dialog::None;
            }
            KeyCode::Char('t') | KeyCode::Char('T') | KeyCode::Enter => {
                self.create_exclusions_template();
            }
            _ => {}
        }
    }

    fn handle_key_disk_space_warning(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                // User wants to continue anyway - start sync (skip further disk checks)
                self.dialog = Dialog::None;
                self.start_sync(true);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                // User cancelled
                self.dialog = Dialog::None;
            }
            _ => {}
        }
    }

    fn handle_key_file_error(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                // Retry - just close dialog, current action will be retried
                self.dialog = Dialog::None;
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                // Skip - mark action as skipped and move to next
                self.skip_current_sync_action();
                self.dialog = Dialog::None;
            }
            KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Esc => {
                // Cancel - abort sync
                if let Some(ref mut syncing) = self.syncing {
                    syncing.cancel_requested = true;
                }
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

    pub(super) fn select_next_project(&mut self) {
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

    pub(super) fn select_previous_project(&mut self) {
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
                    // CopyToLeft means source is RIGHT side
                    // If file exists on right, copy to left
                    // If file doesn't exist on right, delete from left
                    if let Some(size) = preview.get_file_size_from_right(&path) {
                        preview.actions[real_idx] = UserAction::CopyToLeft { path, size };
                    } else {
                        preview.actions[real_idx] = UserAction::DeleteLeft { path };
                    }
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
                    // CopyToRight means source is LEFT side
                    // If file exists on left, copy to right
                    // If file doesn't exist on left, delete from right
                    if let Some(size) = preview.get_file_size_from_left(&path) {
                        preview.actions[real_idx] = UserAction::CopyToRight { path, size };
                    } else {
                        preview.actions[real_idx] = UserAction::DeleteRight { path };
                    }
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

    pub(super) fn open_selected_project(&mut self) {
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
}
