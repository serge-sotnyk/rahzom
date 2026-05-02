//! Console Bridge - injects keystrokes into a child process via WriteConsoleInput
//!
//! Usage: console-bridge.exe <executable> [args...]
//!
//! Commands (written to C:\rahzom-test\.bridge-commands, one per line):
//!   text:hello     - Send text as key events
//!   key:Enter      - Send special key
//!   key:n          - Send single character
//!   capture        - Capture screen to .bridge-screen
//!   exit           - Terminate bridge

use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;
#[cfg(windows)]
use windows::Win32::System::Console::{
    GetConsoleScreenBufferInfo, GetStdHandle, ReadConsoleOutputW, WriteConsoleInputW,
    CHAR_INFO, CONSOLE_SCREEN_BUFFER_INFO, COORD, INPUT_RECORD, KEY_EVENT, KEY_EVENT_RECORD,
    SMALL_RECT, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_RETURN, VK_RIGHT,
    VK_SPACE, VK_TAB, VK_UP, VK_NEXT, VK_PRIOR, VIRTUAL_KEY,
};

const CMD_FILE: &str = r"C:\rahzom-test\.bridge-commands";
const SCREEN_FILE: &str = r"C:\rahzom-test\.bridge-screen";
const POLL_INTERVAL_MS: u64 = 100;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: console-bridge.exe <executable> [args...]");
        eprintln!();
        eprintln!("Commands (write to {}):", CMD_FILE);
        eprintln!("  text:hello     - Send text as key events");
        eprintln!("  key:Enter      - Send special key");
        eprintln!("  key:n          - Send single character");
        eprintln!("  capture        - Capture screen to {}", SCREEN_FILE);
        eprintln!("  exit           - Terminate bridge");
        eprintln!();
        eprintln!("Special keys: Enter, Escape, Tab, BSpace, DC, Up, Down, Left, Right,");
        eprintln!("              Home, End, PageUp, PageDown, Space");
        std::process::exit(1);
    }

    let exe = &args[1];
    let exe_args = &args[2..];

    // Clear command file
    let _ = fs::write(CMD_FILE, "");

    println!("[console-bridge] Starting: {} {:?}", exe, exe_args);
    println!("[console-bridge] Listening for commands on: {}", CMD_FILE);

    // Spawn child process (inherits console)
    let mut child = Command::new(exe)
        .args(exe_args)
        .spawn()
        .with_context(|| format!("Failed to start: {}", exe))?;

    #[cfg(windows)]
    {
        // Get console input handle
        let stdin_handle = unsafe { GetStdHandle(STD_INPUT_HANDLE)? };

        // Main loop: poll command file and inject keystrokes
        loop {
            // Check if child still running
            match child.try_wait() {
                Ok(Some(status)) => {
                    println!("[console-bridge] Child exited with: {}", status);
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("[console-bridge] Error checking child: {}", e);
                    break;
                }
            }

            // Read and process commands
            if let Ok(content) = fs::read_to_string(CMD_FILE) {
                if !content.trim().is_empty() {
                    // Clear file first to avoid re-processing
                    let _ = fs::write(CMD_FILE, "");

                    for line in content.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        if line == "exit" {
                            println!("[console-bridge] Exit command received");
                            let _ = child.kill();
                            return Ok(());
                        }

                        if let Err(e) = process_command(stdin_handle, line) {
                            eprintln!("[console-bridge] Error processing '{}': {}", line, e);
                        }
                    }
                }
            }

            thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }
    }

    #[cfg(not(windows))]
    {
        eprintln!("[console-bridge] This tool only works on Windows");
        let _ = child.wait();
    }

    Ok(())
}

#[cfg(windows)]
fn process_command(handle: HANDLE, cmd: &str) -> Result<()> {
    if let Some(text) = cmd.strip_prefix("text:") {
        // Send text as key events
        for ch in text.chars() {
            send_char(handle, ch)?;
        }
    } else if let Some(key) = cmd.strip_prefix("key:") {
        // Send special key or single character
        send_key(handle, key)?;
    } else if cmd == "capture" {
        // Capture screen to file
        let stdout_handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE)? };
        let screen = capture_screen(stdout_handle)?;
        fs::write(SCREEN_FILE, &screen)?;
        println!("[console-bridge] Screen captured to {}", SCREEN_FILE);
    } else {
        anyhow::bail!("Unknown command format: {}", cmd);
    }
    Ok(())
}

#[cfg(windows)]
fn send_key(handle: HANDLE, key: &str) -> Result<()> {
    // Map key names to virtual key codes
    let (vk, ch): (VIRTUAL_KEY, char) = match key {
        "Enter" => (VK_RETURN, '\r'),
        "Escape" => (VK_ESCAPE, '\x1b'),
        "Tab" => (VK_TAB, '\t'),
        "BSpace" | "Backspace" => (VK_BACK, '\x08'),
        "DC" | "Delete" => (VK_DELETE, '\x7f'),
        "Up" => (VK_UP, '\0'),
        "Down" => (VK_DOWN, '\0'),
        "Left" => (VK_LEFT, '\0'),
        "Right" => (VK_RIGHT, '\0'),
        "Home" => (VK_HOME, '\0'),
        "End" => (VK_END, '\0'),
        "PageUp" => (VK_PRIOR, '\0'),
        "PageDown" => (VK_NEXT, '\0'),
        "Space" => (VK_SPACE, ' '),
        // Single character
        s if s.len() == 1 => {
            let c = s.chars().next().unwrap();
            let vk = char_to_vk(c);
            (vk, c)
        }
        _ => anyhow::bail!("Unknown key: {}", key),
    };

    send_key_event(handle, vk, ch)
}

#[cfg(windows)]
fn send_char(handle: HANDLE, ch: char) -> Result<()> {
    let vk = char_to_vk(ch);
    send_key_event(handle, vk, ch)
}

#[cfg(windows)]
fn char_to_vk(ch: char) -> VIRTUAL_KEY {
    // For printable ASCII, the virtual key code is often the uppercase letter
    // For simplicity, we'll use the character code directly for most cases
    match ch {
        'a'..='z' => VIRTUAL_KEY((ch as u8 - b'a' + b'A') as u16),
        'A'..='Z' => VIRTUAL_KEY(ch as u16),
        '0'..='9' => VIRTUAL_KEY(ch as u16),
        ' ' => VK_SPACE,
        '\r' | '\n' => VK_RETURN,
        '\t' => VK_TAB,
        _ => VIRTUAL_KEY(0), // Let the system figure it out from the char
    }
}

#[cfg(windows)]
fn send_key_event(handle: HANDLE, vk: VIRTUAL_KEY, ch: char) -> Result<()> {
    // Create key down event
    let key_down = INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: windows::Win32::System::Console::INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: true.into(),
                wRepeatCount: 1,
                wVirtualKeyCode: vk.0,
                wVirtualScanCode: 0,
                uChar: windows::Win32::System::Console::KEY_EVENT_RECORD_0 {
                    UnicodeChar: ch as u16,
                },
                dwControlKeyState: 0,
            },
        },
    };

    // Create key up event
    let key_up = INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: windows::Win32::System::Console::INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: false.into(),
                wRepeatCount: 1,
                wVirtualKeyCode: vk.0,
                wVirtualScanCode: 0,
                uChar: windows::Win32::System::Console::KEY_EVENT_RECORD_0 {
                    UnicodeChar: ch as u16,
                },
                dwControlKeyState: 0,
            },
        },
    };

    let events = [key_down, key_up];
    let mut written = 0u32;

    unsafe {
        WriteConsoleInputW(handle, &events, &mut written)
            .context("WriteConsoleInputW failed")?;
    }

    Ok(())
}

#[cfg(windows)]
fn capture_screen(handle: HANDLE) -> Result<String> {
    let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
    unsafe {
        GetConsoleScreenBufferInfo(handle, &mut info).context("GetConsoleScreenBufferInfo failed")?;
    }

    let width = (info.srWindow.Right - info.srWindow.Left + 1) as i16;
    let height = (info.srWindow.Bottom - info.srWindow.Top + 1) as i16;

    // Buffer to hold character and attribute data
    let buffer_size = (width as usize) * (height as usize);
    let mut buffer: Vec<CHAR_INFO> = vec![CHAR_INFO::default(); buffer_size];

    let buffer_coord = COORD { X: width, Y: height };
    let buffer_origin = COORD { X: 0, Y: 0 };
    let mut read_region = SMALL_RECT {
        Left: info.srWindow.Left,
        Top: info.srWindow.Top,
        Right: info.srWindow.Right,
        Bottom: info.srWindow.Bottom,
    };

    unsafe {
        ReadConsoleOutputW(handle, buffer.as_mut_ptr(), buffer_coord, buffer_origin, &mut read_region)
            .context("ReadConsoleOutputW failed")?;
    }

    let mut result = String::new();
    let mut last_attr: u16 = 0xFFFF; // Invalid initial value to force first color output

    for row in 0..height {
        for col in 0..width {
            let idx = (row as usize) * (width as usize) + (col as usize);
            let char_info = &buffer[idx];
            let ch = unsafe { char_info.Char.UnicodeChar };
            let attr = char_info.Attributes;

            // Output ANSI color code if attributes changed
            if attr != last_attr {
                result.push_str(&attr_to_ansi(attr));
                last_attr = attr;
            }

            // Convert UTF-16 to char
            if let Some(c) = char::from_u32(ch as u32) {
                result.push(c);
            } else {
                result.push('?');
            }
        }
        // Reset colors at end of line and add newline
        result.push_str("\x1b[0m");
        // Trim trailing spaces from line
        while result.ends_with(" \x1b[0m") {
            result.truncate(result.len() - 5);
            result.push_str("\x1b[0m");
        }
        result.push('\n');
        last_attr = 0xFFFF; // Reset for next line
    }

    // Final reset
    result.push_str("\x1b[0m");
    Ok(result)
}

#[cfg(windows)]
fn attr_to_ansi(attr: u16) -> String {
    let fg = attr & 0x0F;
    let bg = (attr >> 4) & 0x0F;

    // Map Windows console colors to ANSI codes
    let fg_code = match fg {
        0 => 30,  // Black
        1 => 34,  // Blue
        2 => 32,  // Green
        3 => 36,  // Cyan
        4 => 31,  // Red
        5 => 35,  // Magenta
        6 => 33,  // Yellow/Brown
        7 => 37,  // White/Gray
        8 => 90,  // Bright Black (Gray)
        9 => 94,  // Bright Blue
        10 => 92, // Bright Green
        11 => 96, // Bright Cyan
        12 => 91, // Bright Red
        13 => 95, // Bright Magenta
        14 => 93, // Bright Yellow
        15 => 97, // Bright White
        _ => 37,
    };

    let bg_code = match bg {
        0 => 40,   // Black
        1 => 44,   // Blue
        2 => 42,   // Green
        3 => 46,   // Cyan
        4 => 41,   // Red
        5 => 45,   // Magenta
        6 => 43,   // Yellow/Brown
        7 => 47,   // White/Gray
        8 => 100,  // Bright Black
        9 => 104,  // Bright Blue
        10 => 102, // Bright Green
        11 => 106, // Bright Cyan
        12 => 101, // Bright Red
        13 => 105, // Bright Magenta
        14 => 103, // Bright Yellow
        15 => 107, // Bright White
        _ => 40,
    };

    format!("\x1b[{};{}m", fg_code, bg_code)
}
