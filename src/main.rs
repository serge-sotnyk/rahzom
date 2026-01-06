use anyhow::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use std::io;

mod app;

use app::App;

fn main() -> Result<()> {
    // Initialize terminal with panic hook
    let mut terminal = ratatui::init();

    // Enable mouse capture
    execute!(io::stdout(), EnableMouseCapture)?;

    // Run application
    let result = App::new().run(&mut terminal);

    // Disable mouse capture before restoring
    let _ = execute!(io::stdout(), DisableMouseCapture);

    // Restore terminal
    ratatui::restore();

    result
}
