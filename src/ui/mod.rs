//! TUI components and widgets

pub mod dialogs;
pub mod screens;
pub mod sync_ui;
pub mod widgets;

pub use dialogs::{
    render_cancel_sync_confirm_dialog, render_create_dir_confirm_dialog,
    render_delete_confirm_dialog, render_error_dialog, render_new_project_dialog,
    render_sync_confirm_dialog,
};
pub use screens::{render_preview, render_project_list, render_project_view};
pub use sync_ui::{render_sync_complete, render_syncing};
pub use widgets::{centered_rect, field_style, format_bytes, format_duration};
