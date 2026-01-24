# Specification: Windows TUI Testing Infrastructure for rahzom

## Overview

Infrastructure for automated TUI testing of rahzom on Windows via ConPTY.
Analogous to Linux Docker + tmux, but native for Windows.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      HOST (Windows)                              │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────────┐
│  │                     Claude Code                               │
│  │                                                               │
│  │   PowerShell                                                  │
│  │       │                                                       │
│  │       ▼                                                       │
│  │   pty-exec.ps1 ─────────────────────────────────────┐        │
│  │       │                                              │        │
│  │       ▼                                              │        │
│  │   pty-wrapper.exe run rahzom.exe --size 120x40      │        │
│  │       │                                              │        │
│  │       │  stdin: {"cmd":"send","keys":"j"}           │        │
│  │       │  stdout: {"ok":true}                        │        │
│  │       │                                              │        │
│  └───────┼──────────────────────────────────────────────┼────────┘
│          │                                              │
│  ┌───────┼──────────────────────────────────────────────┼────────┐
│  │       ▼           ConPTY Session                     │        │
│  │   ┌─────────────────────────────────────────────────┐   │        │
│  │   │         rahzom.exe (TUI)                    │   │        │
│  │   │                                             │   │        │
│  │   │   ┌─────────────────────────────────────┐   │   │        │
│  │   │   │  Projects               [+] New     │   │   │        │
│  │   │   │  ► Test Project                     │   │   │        │
│  │   │   │    Another Project                  │   │   │        │
│  │   │   └─────────────────────────────────────┘   │   │        │
│  │   │                                             │   │        │
│  │   └─────────────────────────────────────────────┘   │        │
│  │                                                      │        │
│  │   User: rahzom-tester                               │        │
│  │   Home: C:\rahzom-test                              │        │
│  │                                                      │        │
│  │   C:\rahzom-test\                                   │        │
│  │   ├── left\          ← test data                   │        │
│  │   ├── right\                                        │        │
│  │   ├── .rahzom\       ← config (USERPROFILE)        │        │
│  │   └── bin\                                          │        │
│  │       ├── rahzom.exe                                │        │
│  │       └── pty-wrapper.exe                           │        │
│  │                                                      │        │
│  └──────────────────────────────────────────────────────────────┘
└─────────────────────────────────────────────────────────────────┘
```

## Components

### 1. pty-wrapper.exe

Rust CLI utility for managing ConPTY sessions.

**Usage:**
```powershell
pty-wrapper.exe run <exe> [args] --size <cols>x<rows>
```

**Protocol (JSON via stdin/stdout):**

```jsonc
// Commands (stdin, one per line)
{"cmd": "send", "keys": "j"}              // Send a key
{"cmd": "send", "keys": "Enter"}          // Special keys: Enter, Escape, Tab, BSpace, Left, Right, Up, Down, C-c
{"cmd": "send", "text": "test-project"}   // Send text (for input fields)
{"cmd": "capture"}                         // Get screen contents
{"cmd": "resize", "cols": 100, "rows": 30} // Resize terminal
{"cmd": "exit"}                            // End session

// Responses (stdout)
{"ok": true}
{"ok": true, "screen": "...(ANSI text with colors)..."}
{"ok": false, "error": "Process exited"}
```

**Special Keys:**
| Key | Description |
|-----|-------------|
| Enter | Enter/Return |
| Escape | Escape |
| Tab | Tab |
| BSpace | Backspace |
| DC | Delete |
| Left, Right, Up, Down | Arrow keys |
| Home, End | Home/End |
| PageUp, PageDown | Page Up/Down |
| C-c | Ctrl+C |
| C-d | Ctrl+D |

### 2. rahzom-tester User

Isolated user for tests:
- **Username**: `rahzom-tester`
- **Home**: `C:\rahzom-test`
- **Permissions**: Full access only to `C:\rahzom-test`
- **Groups**: Users (no administrator)

### 3. File Structure

```
C:\rahzom-test\
├── left\           # Test folders (analogous to /test/left in Docker)
├── right\
├── .rahzom\        # rahzom config (projects.json)
└── bin\
    ├── rahzom.exe
    └── pty-wrapper.exe

# Source code (developer repository)
C:\repos\rahzom\
├── src\
├── .claude\
│   └── skills\
│       ├── sandbox-windows-init\
│       │   ├── SKILL.md
│       │   ├── pty-wrapper\     # Cargo crate
│       │   ├── setup-user.ps1
│       │   └── build.ps1
│       └── sandbox-windows-testing\
│           ├── SKILL.md
│           ├── presets.md
│           └── pty-exec.ps1
└── target\release\rahzom.exe
```

---

## Skill: sandbox-windows-init

### Purpose
Initialize Windows environment for TUI testing of rahzom.

### What It Does
1. Checks/creates `rahzom-tester` user
2. Creates folder structure `C:\rahzom-test\`
3. Builds `pty-wrapper.exe`
4. Builds `rahzom.exe`
5. Copies binaries to `C:\rahzom-test\bin\`

### Requirements
- Windows 10/11 with PowerShell 5.1+
- Rust toolchain (rustup)
- Administrator privileges (for user creation)

### Quick Start

```powershell
# 1. Check if user exists
Get-LocalUser -Name "rahzom-tester" -ErrorAction SilentlyContinue

# 2. If not — create (requires Admin)
.\.claude\skills\sandbox-windows-init\setup-user.ps1

# 3. Build pty-wrapper
Push-Location .\.claude\skills\sandbox-windows-init\pty-wrapper
cargo build --release
Pop-Location

# 4. Build rahzom
cargo build --release

# 5. Copy binaries
Copy-Item .\target\release\rahzom.exe C:\rahzom-test\bin\
Copy-Item .\.claude\skills\sandbox-windows-init\pty-wrapper\target\release\pty-wrapper.exe C:\rahzom-test\bin\

# 6. Create test folders
New-Item -ItemType Directory -Force -Path C:\rahzom-test\left, C:\rahzom-test\right

# 7. Verify
C:\rahzom-test\bin\pty-wrapper.exe --version
C:\rahzom-test\bin\rahzom.exe --version
```

### Result
- User `rahzom-tester` exists
- `C:\rahzom-test\bin\pty-wrapper.exe` is ready
- `C:\rahzom-test\bin\rahzom.exe` is ready
- Folders `left\` and `right\` are created

---

## Skill: sandbox-windows-testing

### Purpose
Execute TUI tests for rahzom on Windows.

### Prerequisite
Run `sandbox-windows-init` to prepare the environment.

### Test Workflow

#### 1. Prepare Test Data

```powershell
# Clean up
Remove-Item C:\rahzom-test\left\* -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item C:\rahzom-test\right\* -Recurse -Force -ErrorAction SilentlyContinue

# basic_sync preset
Set-Content C:\rahzom-test\left\shared.txt -Value "shared content"
Set-Content C:\rahzom-test\right\shared.txt -Value "shared content"
Set-Content C:\rahzom-test\left\left_only.txt -Value "left only content"
Set-Content C:\rahzom-test\right\right_only.txt -Value "right only content"
```

#### 2. Clear rahzom Config (fresh start)

```powershell
Remove-Item C:\rahzom-test\.rahzom -Recurse -Force -ErrorAction SilentlyContinue
```

#### 3. Launch rahzom via pty-wrapper

```powershell
# Launch (blocks terminal, communication via stdin/stdout)
$env:USERPROFILE = "C:\rahzom-test"
$process = Start-Process -FilePath "C:\rahzom-test\bin\pty-wrapper.exe" `
    -ArgumentList "run", "C:\rahzom-test\bin\rahzom.exe", "--size", "120x40" `
    -NoNewWindow -PassThru -RedirectStandardInput stdin.txt -RedirectStandardOutput stdout.txt

# Or use pty-exec.ps1 for interactive control
```

#### 4. Send Commands and Get Screen

Use the `pty-exec.ps1` wrapper:

```powershell
# Start session
$session = .\.claude\skills\sandbox-windows-testing\pty-exec.ps1 -Start

# Wait for startup
Start-Sleep -Seconds 1

# Get screen
$screen = .\.claude\skills\sandbox-windows-testing\pty-exec.ps1 -Capture
Write-Host $screen

# Send key
.\.claude\skills\sandbox-windows-testing\pty-exec.ps1 -Send "j"

# Send text
.\.claude\skills\sandbox-windows-testing\pty-exec.ps1 -SendText "test-project"

# Exit
.\.claude\skills\sandbox-windows-testing\pty-exec.ps1 -Exit
```

#### 5. Verify Results

```powershell
# Files
Get-ChildItem C:\rahzom-test\left\
Get-ChildItem C:\rahzom-test\right\

# Metadata
Get-ChildItem C:\rahzom-test\left\.rahzom -ErrorAction SilentlyContinue
```

### Example: Creating a Project and Analysis

```powershell
# Preparation
Remove-Item C:\rahzom-test\.rahzom -Recurse -Force -ErrorAction SilentlyContinue
Set-Content C:\rahzom-test\left\file1.txt -Value "content"

# Launch
$session = .\pty-exec.ps1 -Start
Start-Sleep -Seconds 1

# Press 'n' for new project
.\pty-exec.ps1 -Send "n"
Start-Sleep -Milliseconds 500

# Enter project name
.\pty-exec.ps1 -SendText "test-project"
.\pty-exec.ps1 -Send "Tab"

# Enter left path
.\pty-exec.ps1 -SendText "C:\rahzom-test\left"
.\pty-exec.ps1 -Send "Tab"

# Enter right path
.\pty-exec.ps1 -SendText "C:\rahzom-test\right"
.\pty-exec.ps1 -Send "Enter"

Start-Sleep -Milliseconds 500

# Check screen
$screen = .\pty-exec.ps1 -Capture
Write-Host $screen

# Exit
.\pty-exec.ps1 -Send "q"
.\pty-exec.ps1 -Exit
```

### Key Differences from Linux

| Aspect | Linux (Docker + tmux) | Windows (ConPTY) |
|--------|----------------------|------------------|
| Isolation | Docker container | Separate user + folder |
| Terminal | tmux session | ConPTY via pty-wrapper |
| Send keys | `tmux send-keys` | `pty-wrapper send` |
| Screen capture | `tmux capture-pane -e` | `pty-wrapper capture` |
| Test folders | `/test/left`, `/test/right` | `C:\rahzom-test\left`, `right` |
| rahzom config | `/root/.rahzom` | `C:\rahzom-test\.rahzom` |
| Execution | `docker exec` | PowerShell directly |

### Presets

See [presets.md](presets.md) — adapted versions of Linux presets for Windows paths.

---

## pty-wrapper: Implementation Details

### Cargo.toml

```toml
[package]
name = "pty-wrapper"
version = "0.1.0"
edition = "2021"

[dependencies]
portable-pty = "0.8"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
```

### Core Logic

1. **Startup**: Creates ConPTY with specified size, launches process
2. **Loop**: Reads JSON commands from stdin, executes, writes result to stdout
3. **Screen buffer**: Stores last PTY output (with ANSI codes)
4. **Termination**: On `exit` command or when child process ends

### Special Key Handling

```rust
fn key_to_bytes(key: &str) -> Vec<u8> {
    match key {
        "Enter" => vec![0x0D],
        "Escape" => vec![0x1B],
        "Tab" => vec![0x09],
        "BSpace" => vec![0x7F],
        "DC" => vec![0x1B, 0x5B, 0x33, 0x7E],  // ESC[3~
        "Up" => vec![0x1B, 0x5B, 0x41],         // ESC[A
        "Down" => vec![0x1B, 0x5B, 0x42],       // ESC[B
        "Right" => vec![0x1B, 0x5B, 0x43],      // ESC[C
        "Left" => vec![0x1B, 0x5B, 0x44],       // ESC[D
        "C-c" => vec![0x03],
        "C-d" => vec![0x04],
        other => other.as_bytes().to_vec(),
    }
}
```

---

## Open Questions

### 1. USERPROFILE for rahzom
rahzom uses `dirs::home_dir()` for `~/.rahzom`. On Windows this is `%USERPROFILE%`.

**Options:**
- A) Run pty-wrapper with `$env:USERPROFILE = "C:\rahzom-test"` — simple hack
- B) Add `--config-dir` flag to rahzom — cleaner, but requires changes
- C) Run as rahzom-tester via `runas` — full isolation

**Recommendation**: Start with A, switch to B if needed.

### 2. Output Buffering
ConPTY buffers output. A delay is needed between `send` and `capture`.

**Solution**: Wait ~100-500ms after each `send` before `capture`.

### 3. ANSI Colors
Windows Terminal supports ANSI. But if rahzom runs through ConPTY directly (without Windows Terminal), ANSI codes must be verified to pass through.

**Solution**: `portable-pty` should correctly pass ANSI. Verify during implementation.

---

## Next Steps

1. [ ] Implement pty-wrapper (Rust)
2. [ ] Write setup-user.ps1
3. [ ] Write pty-exec.ps1 wrapper
4. [ ] Adapt presets.md for Windows
5. [ ] Write SKILL.md for both skills
6. [ ] Test full workflow
