---
name: sandbox-windows-testing
description: Execute TUI tests for rahzom on Windows using shared console. Use for testing UI interactions, keyboard navigation, screen rendering, sync workflows. Requires sandbox-windows-init to be run first.
---

# Windows Sandbox Testing (Shared Console)

Execute TUI tests for rahzom using the shared console approach - user sees everything in real-time.

**Prerequisite**: Run `/sandbox-windows-init` first to set up the environment.

## Starting the Test Console

Ask the user to:

1. Open a new cmd window
2. Run: `runas /user:rahzom-tester "cmd /k cd C:\rahzom-test && bin\console-bridge.exe bin\rahzom.exe"`
3. Enter the password when prompted

The user will see rahzom TUI start in the new console.

## Rahzom Keyboard Shortcuts

| Screen | Key | Action |
|--------|-----|--------|
| Projects | `n` | New project |
| Projects | `d` | Delete project |
| Projects | `Enter` | Open project |
| Projects | `q` | Quit |
| Preview | `a` | Analyze (scan folders) |
| Preview | `g` | **Go** (execute sync) |
| Preview | `j`/`k` or `↑`/`↓` | Navigate items |
| Preview | `←`/`→` | Change action for selected item |
| Preview | `s` | **Skip** (set action to Skip) |
| Preview | `Escape` | Back to projects |
| Dialogs | `Tab` | Next field |
| Dialogs | `Enter` | Confirm |
| Dialogs | `Escape` | Cancel |

## Prepare Test Data

Choose a preset from [presets.md](presets.md) or create custom data:

```powershell
# Clear previous test data
Remove-Item "C:\rahzom-test\left\*" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "C:\rahzom-test\right\*" -Recurse -Force -ErrorAction SilentlyContinue

# Example: basic_sync preset
Set-Content "C:\rahzom-test\left\shared.txt" -Value "shared content"
Set-Content "C:\rahzom-test\right\shared.txt" -Value "shared content"
Set-Content "C:\rahzom-test\left\left_only.txt" -Value "left only content"
Set-Content "C:\rahzom-test\right\right_only.txt" -Value "right only content"
```

## Clear Config (fresh start)

```powershell
Remove-Item "C:\rahzom-test\.rahzom" -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\.rahzom" | Out-Null
```

## Bridge Commands

Write commands to `C:\rahzom-test\.bridge-commands`:

```powershell
# Send single key
Add-Content "C:\rahzom-test\.bridge-commands" "key:n"

# Send text
Add-Content "C:\rahzom-test\.bridge-commands" "text:my-project"

# Send special keys
Add-Content "C:\rahzom-test\.bridge-commands" "key:Enter"
Add-Content "C:\rahzom-test\.bridge-commands" "key:Tab"
Add-Content "C:\rahzom-test\.bridge-commands" "key:Escape"

# Exit bridge (terminates rahzom)
Add-Content "C:\rahzom-test\.bridge-commands" "exit"
```

## Command Reference

| Command | Description |
|---------|-------------|
| `key:Enter` | Press Enter |
| `key:Escape` | Press Escape |
| `key:Tab` | Press Tab |
| `key:BSpace` | Press Backspace |
| `key:Up` | Arrow up |
| `key:Down` | Arrow down |
| `key:Left` | Arrow left |
| `key:Right` | Arrow right |
| `key:Home` | Home key |
| `key:End` | End key |
| `key:Space` | Space key |
| `key:n` | Single character (any letter/number) |
| `text:hello` | Type text "hello" |
| `capture` | Capture screen to `.bridge-screen` |
| `exit` | Terminate bridge and rahzom |

## Screen Capture

Claude can "see" the screen by capturing it:

```powershell
# Request screen capture
Add-Content "C:\rahzom-test\.bridge-commands" "capture"
Start-Sleep -Milliseconds 200

# Read the captured screen (includes ANSI colors)
Get-Content "C:\rahzom-test\.bridge-screen" -Raw
```

Use Read tool to view `C:\rahzom-test\.bridge-screen` with ANSI colors rendered.

## Common Test Scenarios

### Create new project and analyze

```powershell
# Prepare data
Remove-Item "C:\rahzom-test\.rahzom" -Recurse -Force -ErrorAction SilentlyContinue
Set-Content "C:\rahzom-test\left\file1.txt" -Value "content"

# Wait for user to start console-bridge, then:

# Press 'n' for new project
Add-Content "C:\rahzom-test\.bridge-commands" "key:n"
Start-Sleep -Milliseconds 500

# Enter project name
Add-Content "C:\rahzom-test\.bridge-commands" "text:test-project"
Add-Content "C:\rahzom-test\.bridge-commands" "key:Tab"

# Enter left path
Add-Content "C:\rahzom-test\.bridge-commands" "text:C:\rahzom-test\left"
Add-Content "C:\rahzom-test\.bridge-commands" "key:Tab"

# Enter right path
Add-Content "C:\rahzom-test\.bridge-commands" "text:C:\rahzom-test\right"
Add-Content "C:\rahzom-test\.bridge-commands" "key:Enter"

Start-Sleep -Milliseconds 500

# User sees project created in their console
```

### Navigate and sync

```powershell
# Select project (if not already selected)
Add-Content "C:\rahzom-test\.bridge-commands" "key:Enter"
Start-Sleep -Milliseconds 300

# Press 'a' to analyze
Add-Content "C:\rahzom-test\.bridge-commands" "key:a"
Start-Sleep -Seconds 1

# Navigate with j/k
Add-Content "C:\rahzom-test\.bridge-commands" "key:j"
Add-Content "C:\rahzom-test\.bridge-commands" "key:j"

# Change action with arrows
Add-Content "C:\rahzom-test\.bridge-commands" "key:Right"

# Press 'g' to Go (execute sync)
Add-Content "C:\rahzom-test\.bridge-commands" "key:g"
```

### Exit rahzom

```powershell
# Press 'q' to quit rahzom
Add-Content "C:\rahzom-test\.bridge-commands" "key:q"

# Or force terminate bridge
Add-Content "C:\rahzom-test\.bridge-commands" "exit"
```

## Verifying Results

After test actions, verify by:

1. **Screen capture**: Use `capture` command and read `.bridge-screen`
2. **File system**: Check files were synced correctly

```powershell
# Check files
Get-ChildItem "C:\rahzom-test\left"
Get-ChildItem "C:\rahzom-test\right"

# Check metadata
Get-ChildItem "C:\rahzom-test\left\.rahzom" -ErrorAction SilentlyContinue
```

## Tips

- Add `Start-Sleep -Milliseconds 300` between commands for UI to update
- Use `capture` command after each action to see the result
- User can intervene manually by typing in the console
- User can close the console window to terminate at any time
- Clear `.bridge-commands` file if corrupted: `Set-Content "C:\rahzom-test\.bridge-commands" ""`
