# rahzom — Requirements Specification (MVP)

> Lightweight cross-platform folder synchronization utility.  
> Name from Ukrainian "разом" (razom = together) — files always together, synchronized.

---

## 1. Project Overview

### 1.1 Purpose
A portable two-way folder synchronization tool, alternative to GoodSync with focus on simplicity and portability.

### 1.2 Target Platforms
- Windows (primary)
- macOS
- Linux

### 1.3 Technology Stack
- Language: Rust
- UI: TUI (Ratatui + crossterm)
- License: MIT

### 1.4 Key Characteristics
- Single portable binary (~5-10 MB)
- No runtime dependencies
- No installer required
- Mouse support in terminal

---

## 2. Core Concepts

### 2.1 Project
A **project** is a pair of folders to synchronize, bound to a specific computer.

- Each project has a unique name (e.g., `docs`, `med`)
- Projects are stored locally in `~/.rahzom/`
- Different computers may have different projects or same project names with different paths
- Example: Desktop has project `docs` pointing to `C:\docs ↔ F:\docs`, Laptop has project `docs` pointing to `/home/user/docs ↔ /media/usb/docs`

### 2.2 Metadata Folder
Each synchronized folder contains a hidden `.rahzom/` directory storing:

- File state database (for change detection)
- Deleted files registry
- Backup copies of overwritten/deleted files
- Synchronization logs
- Exclusion rules

The `.rahzom/` folder is **not synchronized** between sides — each side maintains its own independently.

### 2.3 Synchronization Model
Two-way synchronization with explicit user control:

1. **Analyze** — scan both sides, detect changes, generate action plan
2. **Preview** — user reviews and optionally modifies proposed actions
3. **Sync** — execute approved actions

---

## 3. Change Detection

### 3.1 What Constitutes a Change
A file is considered **changed** if any of the following differ from last known state:
- Modification time (mtime)
- File size
- Content hash (optional, user-configurable)

### 3.2 Change Types
| State | Left Side | Right Side | Detection |
|-------|-----------|------------|-----------|
| New | Exists, not in metadata | Not exists | Copy to right |
| New | Not exists | Exists, not in metadata | Copy to left |
| Modified | Changed since last sync | Unchanged | Copy to right |
| Modified | Unchanged | Changed since last sync | Copy to left |
| Deleted | Was in metadata, now gone | Exists unchanged | Delete from right |
| Deleted | Exists unchanged | Was in metadata, now gone | Delete from left |
| Conflict | Changed | Changed | User must decide |
| Conflict | Deleted | Changed | User must decide |
| Conflict | Changed | Deleted | User must decide |

### 3.3 First Synchronization
When no metadata exists (fresh start):
- Typical scenario: clean USB drive, copy files from computer
- If both sides have files: treat each file as potential conflict, ask user
- Option for auto-strategy (e.g., "newer wins") can be added later

### 3.4 Hash Calculation
- Optional feature, configurable per project
- If enabled, hash stored in metadata
- Recalculate only when size or mtime changed
- Algorithm: SHA-256 (or configurable)

### 3.5 FAT32 Time Precision
FAT32 filesystem has 2-second mtime precision. When comparing:
- Allow ±2 second tolerance to avoid false positives
- Detect filesystem type and adjust tolerance accordingly

---

## 4. Conflict Resolution

### 4.1 Definition
A **conflict** occurs when a file is modified (including deleted) on both sides since last successful synchronization.

### 4.2 Resolution Strategy
- Always show conflicts to user
- No automatic resolution — user explicitly chooses action
- Available actions per file:
  - Copy left → right
  - Copy right → left
  - Skip (do nothing)
  - Delete both sides
  - Delete left only
  - Delete right only

### 4.3 Default Action Proposal
- If file modified on one side, unchanged on other → propose copy from modified side
- If file deleted on one side, other side unchanged and matches last known state → propose delete
- If both sides changed → no default action, user must decide (shown as "undefined")

---

## 5. Data Safety

### 5.1 Backup Before Overwrite/Delete
- Before overwriting or deleting a file, save copy to `.rahzom/_backup/`
- Configurable: keep last N versions (default: 5)
- Backup retention is project setting stored in `~/.rahzom/`

### 5.2 Soft Delete
- When synchronizing a deletion, move file to `.rahzom/_trash/` instead of permanent delete
- User can recover from trash manually
- Trash cleanup: configurable retention period

### 5.3 Deleted Files Registry
- Store metadata about deleted files for 90 days (configurable)
- Stored info: path, size, mtime, hash, deletion date
- Enables proper deletion propagation across sync points
- Cleanup: remove entries older than retention period

### 5.4 Idempotency
Synchronization should be safely repeatable:
- If interrupted, can be re-run without data loss
- Partial state should not corrupt future syncs
- Local disk always serves as "source of truth" in disaster scenarios

---

## 6. File Operations

### 6.1 Copy Operation
- Preserve original modification time (mtime)
- Store virtual attributes in metadata (see 6.3)
- Apply platform-specific attributes where meaningful

### 6.2 Operation Order
1. Create folders (depth-first)
2. Copy/update files
3. Delete files
4. Delete empty folders (leaf-first)

### 6.3 File Attributes
Store virtual attributes in metadata for cross-platform compatibility:
- Unix: permissions (rwx), owner, group
- Windows: readonly, hidden, system

On synchronization:
- Write attributes meaningful for target platform
- Ignore attributes that don't translate (e.g., Unix permissions on FAT32)

### 6.4 Rename/Move Detection
For MVP: treat as two operations (delete + create new).
- Simple and safe
- Inefficient for large files, but correct
- Future optimization: detect by hash match

### 6.5 Symbolic Links
For MVP: **skip with warning**.
- Log: "Skipped symlink: path/to/link"
- Avoids infinite loops and broken links
- Future: configurable handling modes

### 6.6 Empty Folders
- Synchronize folder structure
- Keep empty folders (do not auto-delete)

---

## 7. Filtering and Exclusions

### 7.1 Exclusion Rules Storage
Exclusion rules stored in `.rahzom/exclusions.conf` (or similar).
- Synchronized between sides (lives in `.rahzom/` which is on both sides)
- Edit on one computer → available on all after sync

### 7.2 Exclusion Types
- **Pattern-based**: glob patterns like `*.tmp`, `.git/`, `node_modules/`, `Thumbs.db`, `__pycache__/`
- **Size-based**: skip files larger than N MB
- **Hidden/system files**: configurable (sync or ignore)

### 7.3 Default Exclusions
Suggested defaults (user can modify):
```
.DS_Store
Thumbs.db
*.tmp
*.temp
~*
```

---

## 8. Error Handling

### 8.1 File Locked/In Use (Windows)
- Pause synchronization
- Show dialog: Retry / Skip / Cancel sync
- If retry: verify file unchanged before proceeding

### 8.2 Insufficient Disk Space
- Pre-check: estimate required space before sync
- If not enough: warn user, allow to proceed anyway
- On write error: pause, show dialog, allow user to free space and continue

### 8.3 Permission Denied
- Pause synchronization
- Show dialog: Skip / Cancel sync
- Log the error

### 8.4 File Changed During Sync
- Before each copy operation, verify source file (size/mtime)
- If changed: skip, add to "changed during sync" list
- After sync: show list, suggest re-analyze

### 8.5 Long Paths (Windows)
- Support paths >260 characters via `\\?\` prefix
- Rust's std::fs handles this with proper configuration

### 8.6 Invalid Filenames Cross-Platform
- Windows forbidden: `\ / : * ? " < > |`
- If file from Linux has such characters: skip with warning
- Log: "Skipped due to invalid filename: path/to/file"

### 8.7 Case Sensitivity
- Store original filename case in metadata
- Compare filenames case-insensitively
- If Linux folder has `file.txt` and `File.txt`: warn about conflict, skip both

---

## 9. User Interface

### 9.1 Application Flow
```
┌─────────────────────────────────────────┐
│           STARTUP SCREEN                │
│  ┌───────────────────────────────────┐  │
│  │  Recent Projects:                 │  │
│  │    > docs                         │  │
│  │      med                          │  │
│  │                                   │  │
│  │  [N] New Project                  │  │
│  │  [Q] Quit                         │  │
│  └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
           │
           ▼
┌─────────────────────────────────────────┐
│           PROJECT VIEW                  │
│  Project: docs                          │
│  Left:  C:\Users\me\docs                │
│  Right: F:\docs                         │
│  ─────────────────────────────────────  │
│  [A] Analyze  [S] Sync  [C] Config      │
│  ─────────────────────────────────────  │
│  Status: Not analyzed yet               │
│                                         │
│  [Esc] Back to menu                     │
└─────────────────────────────────────────┘
           │
           ▼ (after Analyze)
┌─────────────────────────────────────────┐
│           PREVIEW / FILE LIST           │
│  Filter: [All] [Changes] [Conflicts]    │
│  ─────────────────────────────────────  │
│  Name              L    R    Action     │
│  ─────────────────────────────────────  │
│  ▼ documents/      ○    ○              │
│    report.docx   12KB  ──►  Copy →     │
│    notes.txt       ◄──  8KB  Copy ←    │
│  ▼ photos/         ○    ○              │
│    img001.jpg      ✕   15KB  Delete?   │
│  ─────────────────────────────────────  │
│  Total: 3 changes, 150 KB to transfer   │
│  [S] Sync  [Esc] Cancel                 │
└─────────────────────────────────────────┘
```

### 9.2 Navigation
- Arrow keys: move cursor
- Enter: expand/collapse folder, confirm selection
- Escape: go back / cancel
- Tab: switch between panels/elements

### 9.3 Hotkeys
| Key | Action |
|-----|--------|
| A | Analyze |
| S | Start Sync |
| Q | Quit |
| Space | Select/deselect file |
| Left Arrow | Set action: copy left |
| Right Arrow | Set action: copy right |
| D | Set action: skip/disable |
| Delete | Set action: delete |
| F | Cycle filters |
| ? | Help |

### 9.4 Mouse Support
- Click: select item
- Double-click: expand/collapse folder
- Right-click: context menu (optional for MVP)
- Scroll: navigate list

### 9.5 Action Indicators (Unicode)
| Symbol | Meaning |
|--------|---------|
| `──►` or `→` | Copy to right |
| `◄──` or `←` | Copy to left |
| `◄─►` or `↔` | Conflict (user must choose) |
| `✕` | Delete |
| `═` | Unchanged/synced |
| `?` | Action undefined |

### 9.6 Progress Display
During synchronization show:
- Current file being processed
- Files processed / total files
- Bytes transferred / total bytes
- Estimated time remaining
- Elapsed time

### 9.7 Filters in Preview
Minimum for MVP:
- **All**: show all files
- **Changes**: show only files with pending actions

Nice to have:
- New files only
- Conflicts only
- By direction (left→right, right→left)

---

## 10. Project Configuration

### 10.1 Storage Location
- Global config: `~/.rahzom/config.toml`
- Projects list: `~/.rahzom/projects/`
- Each project: `~/.rahzom/projects/{project_name}.toml`

### 10.2 Project Settings
```toml
[project]
name = "docs"
left_path = "C:\\Users\\me\\docs"
right_path = "F:\\docs"

[sync]
verify_hash = false
backup_versions = 5

[filters]
# Exclusions stored in .rahzom/exclusions on each side
```

### 10.3 Creating New Project
1. Enter project name (auto-suggest from folder name)
2. Select left folder (text input + file browser button)
3. Select right folder (text input + file browser button)
4. If folder doesn't exist: offer to create
5. Save project

### 10.4 Editing Project
- Change paths (useful when drive letter changes)
- Adjust backup settings
- Configure hash verification

---

## 11. Metadata Storage

### 11.1 Location
Inside each synchronized folder: `.rahzom/`

### 11.2 Structure
```
.rahzom/
├── state.db          # File state database
├── exclusions.conf   # Exclusion patterns
├── _backup/          # Backup copies before overwrite
│   └── {filename}.{timestamp}
├── _trash/           # Soft-deleted files
│   └── {filename}.{timestamp}
└── logs/             # Sync logs
    └── sync-2026-01-04-174036.log
```

### 11.3 State Database
Format: SQLite (for performance with large file counts) or JSON (for simplicity).

Recommendation: Start with JSON for MVP, migrate to SQLite if performance issues arise.

Per-file record:
```json
{
  "path": "documents/report.docx",
  "size": 12543,
  "mtime": "2026-01-04T15:30:00Z",
  "hash": "sha256:abc123...",  // optional
  "attributes": {
    "unix_mode": "0644",
    "windows_readonly": false
  },
  "last_synced": "2026-01-04T17:40:00Z"
}
```

Deleted files registry:
```json
{
  "path": "old/removed.txt",
  "size": 1024,
  "mtime": "2025-12-01T10:00:00Z",
  "hash": "sha256:def456...",
  "deleted_at": "2026-01-04T17:40:00Z"
}
```

### 11.4 Log Format
Plain text, one entry per line:
```
2026-01-04 17:40:36 [INFO] Sync started
2026-01-04 17:40:36 [COPY] documents/report.docx → right (12543 bytes)
2026-01-04 17:40:37 [COPY] notes.txt ← left (8192 bytes)
2026-01-04 17:40:37 [DELETE] old/removed.txt (right)
2026-01-04 17:40:38 [INFO] Sync completed: 3 operations, 0 errors
```

### 11.5 Log Rotation
- Keep last 10 log files
- Auto-delete older logs on startup

---

## 12. Logging and Reporting

### 12.1 Log Levels
For MVP: log everything (no level filtering).

Future: configurable verbosity.

### 12.2 Log Window in TUI
- Show real-time log during operations
- Scrollable
- Can be collapsed/expanded

### 12.3 Post-Sync Summary
Display after sync completes:
```
Synchronization completed
─────────────────────────
Files copied:     15
Files deleted:    2
Bytes transferred: 1.2 MB
Time elapsed:     00:00:05
Errors:           0

Press any key to continue
```

---

## 13. Performance

### 13.1 MVP Approach
- Sequential scanning (no parallelism)
- Sequential file operations
- Optimize later if needed

### 13.2 Scale Target
- Support tens of thousands of files
- Acceptable scan time: under 1 minute for 50,000 files on SSD

### 13.3 Progress Feedback
- Update progress every 100ms or every 100 files (whichever comes first)
- Show current file being processed

---

## 14. Versioning

### 14.1 Metadata Format Version
- No explicit version migration for MVP
- If breaking changes needed: change folder name (`.rahzom_v2/`)
- Old metadata simply ignored, fresh start

### 14.2 Application Version
- Semantic versioning: MAJOR.MINOR.PATCH
- Display in UI: `rahzom v0.1.0`

---

## 15. Internationalization

### 15.1 MVP Language
English only.

### 15.2 Character Encoding
- All internal strings: UTF-8
- Filenames: read as-is from OS
- Display issues with non-UTF-8: show raw bytes, don't crash

---

## 16. Out of Scope for MVP

The following features are explicitly deferred:

- GUI version (Tauri)
- Cloud sync (Google Drive, Dropbox)
- Encryption
- Automatic sync on device connect
- Code signing
- CLI/headless mode
- Scheduled synchronization
- Network folder sync (SMB/NFS)
- Rename/move detection optimization
- Parallel file operations
- Multi-language support

---

## 17. Success Criteria for MVP

MVP is complete when:

1. ✓ User can create a project (pair of folders)
2. ✓ User can analyze differences between folders
3. ✓ Changes displayed in tree view with proposed actions
4. ✓ User can modify actions for individual files
5. ✓ User can execute synchronization
6. ✓ Conflicts detected and shown for user resolution
7. ✓ Deletions properly propagated (with soft delete)
8. ✓ Backups created before overwrite
9. ✓ Works on Windows with long paths
10. ✓ Works on Linux and macOS
11. ✓ Mouse support in TUI
12. ✓ Exclusion patterns configurable

---

## Appendix A: Glossary

| Term | Definition |
|------|------------|
| Project | A saved configuration pairing two folders for sync |
| Left/Right | The two sides of synchronization (arbitrary naming) |
| Analyze | Scan both sides and compute required actions |
| Sync | Execute the synchronization actions |
| Conflict | File changed on both sides since last sync |
| Metadata | State information stored in `.rahzom/` folder |
| Sidecar | The `.rahzom/` folder alongside synchronized data |

---

## Appendix B: File State Diagram

```
                    ┌─────────────┐
                    │   Unknown   │
                    │ (no metadata)│
                    └──────┬──────┘
                           │ First sync
                           ▼
┌──────────────┐    ┌─────────────┐    ┌──────────────┐
│   Modified   │◄───│   Synced    │───►│   Deleted    │
│ (local change)│    │  (in sync)  │    │(removed local)│
└──────┬───────┘    └──────┬──────┘    └──────┬───────┘
       │                   │                   │
       │    ┌──────────────┴──────────────┐   │
       │    │      Remote also changed     │   │
       │    ▼                              ▼   │
       │ ┌─────────────┐           ┌──────────┴──┐
       └►│  Conflict   │           │ Propagate   │
         │(both changed)│           │  deletion   │
         └─────────────┘           └─────────────┘
```
