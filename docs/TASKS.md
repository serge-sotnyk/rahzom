# rahzom — Implementation Plan (MVP)

> Core-first approach with TDD.  
> Each stage ends with runnable/testable result.

---

## Overview

| Stage | Name | Estimated Time | Deliverable |
|-------|------|----------------|-------------|
| 0 | Project Setup | 2-3 hours | Compiling project, CI, test generator |
| 1 | File Scanner | 0.5-1 day | Scan folder → file tree structure |
| 2 | Metadata Storage | 0.5-1 day | Save/load file state to `.rahzom/` |
| 3 | Differ | 0.5-1 day | Compare two states → list of actions |
| 4 | Executor | 0.5-1 day | Execute copy/delete operations |
| 5 | Project Config | 0.5 day | Load/save projects from `~/.rahzom/` |
| 6 | TUI Shell | 0.5 day | Basic TUI app that starts and exits |
| 7 | TUI: Project List | 0.5-1 day | Startup screen with project selection |
| 8 | TUI: Analyze & Preview | 1-1.5 days | Tree view with proposed actions |
| 9 | TUI: Sync Execution | 0.5-1 day | Progress display, sync execution |
| 9b | Refactoring | 0.5 day | Split app.rs into modules |
| 10 | Filtering & Exclusions | 0.5 day | Pattern-based exclusions |
| 10b | Simplify Exclusions | 0.25 day | Move to .rahzomignore, opt-in |
| 10c | Fix Direction Change | 0.1 day | Delete action for one-sided files |
| 11 | Error Handling & Edge Cases | 1 day | Locked files, long paths, etc. |
| 12 | Polish & Integration Tests | 1 day | End-to-end tests, README |

**Total estimate: 8-12 days of focused work**

---

## Versioning Convention

At the end of each stage, increment the **MINOR** version in `Cargo.toml`:
- Stage 0 → `0.1.0`
- Stage 1 → `0.2.0`
- Stage 2 → `0.3.0`
- ... and so on until release `1.0.0`

---

## Stage 0: Project Setup

**Goal**: Working Rust project with tests infrastructure and test data generator.

### Tasks

#### 0.1 Initialize Cargo project
- `cargo new rahzom`
- Add dependencies to `Cargo.toml`:
  ```toml
  [dependencies]
  ratatui = "0.29"
  crossterm = "0.28"
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  walkdir = "2"
  chrono = { version = "0.4", features = ["serde"] }
  anyhow = "1"
  sha2 = "0.10"           # For SHA-256 hashing
  dirs = "5"              # For ~/.rahzom path
  
  [dev-dependencies]
  tempfile = "3"          # Temporary directories for tests
  ```

#### 0.2 Create project structure
```
rahzom/
├── src/
│   ├── main.rs
│   ├── lib.rs            # Re-exports for testing
│   ├── sync/
│   │   ├── mod.rs
│   │   ├── scanner.rs
│   │   ├── differ.rs
│   │   ├── executor.rs
│   │   └── metadata.rs
│   ├── config/
│   │   ├── mod.rs
│   │   └── project.rs
│   └── ui/
│       └── mod.rs
├── tests/
│   ├── common/
│   │   └── mod.rs        # Test utilities & generators
│   └── integration/
│       └── mod.rs
└── Cargo.toml
```

#### 0.3 Test data generator
Create helper functions in `tests/common/mod.rs`:
```rust
// Creates temp directory with predefined file structure
pub fn create_test_tree(spec: &TreeSpec) -> TempDir

// Example specs:
// - empty folder
// - folder with files (various sizes, dates)
// - nested folders
// - files with unicode names
// - files with long paths (Windows test)
```

#### 0.4 Setup GitHub Actions (optional but recommended)
- Build on push
- Run tests
- Build for Windows/Linux/macOS

### Definition of Done
- [x] `cargo build` succeeds
- [x] `cargo test` runs (even with 0 tests)
- [x] Test generator creates temp folder with files
- [x] `.gitignore` configured

---

## Stage 1: File Scanner

**Goal**: Scan a directory and produce structured representation of all files.

### Tasks

#### 1.1 Define core data structures
```rust
// src/sync/scanner.rs

pub struct FileEntry {
    pub path: PathBuf,        // Relative to sync root
    pub size: u64,
    pub mtime: DateTime<Utc>,
    pub is_dir: bool,
    pub hash: Option<String>, // Computed on demand
}

pub struct ScanResult {
    pub root: PathBuf,
    pub entries: Vec<FileEntry>,
    pub scan_time: DateTime<Utc>,
}
```

#### 1.2 Implement scanner
- Use `walkdir` for recursive traversal
- Handle errors gracefully (permission denied → skip with warning)
- Skip `.rahzom/` directory
- Support long paths on Windows (`\\?\` prefix)

#### 1.3 Add hash computation (optional, on-demand)
- SHA-256
- Only compute when explicitly requested
- Stream-based (don't load entire file to memory)

#### 1.4 Write tests
- Empty directory
- Flat directory with files
- Nested directory structure
- Directory with `.rahzom/` (should be skipped)
- Files with various mtime values
- Error handling (unreadable file)

### Definition of Done
- [x] `Scanner::scan(path) -> Result<ScanResult>`
- [x] Tests pass for all scenarios
- [x] Can scan real folder with thousands of files (manual test)

---

## Stage 2: Metadata Storage

**Goal**: Persist and load file state to/from `.rahzom/` directory.

### Tasks

#### 2.1 Define metadata structures
```rust
// src/sync/metadata.rs

pub struct FileState {
    pub path: String,
    pub size: u64,
    pub mtime: DateTime<Utc>,
    pub hash: Option<String>,
    pub attributes: FileAttributes,
    pub last_synced: DateTime<Utc>,
}

pub struct DeletedFile {
    pub path: String,
    pub size: u64,
    pub mtime: DateTime<Utc>,
    pub hash: Option<String>,
    pub deleted_at: DateTime<Utc>,
}

pub struct SyncMetadata {
    pub files: Vec<FileState>,
    pub deleted: Vec<DeletedFile>,
    pub last_sync: Option<DateTime<Utc>>,
}

pub struct FileAttributes {
    pub unix_mode: Option<u32>,
    pub windows_readonly: Option<bool>,
    pub windows_hidden: Option<bool>,
}
```

#### 2.2 Implement storage operations
- `Metadata::load(rahzom_path) -> Result<SyncMetadata>`
- `Metadata::save(rahzom_path, &SyncMetadata) -> Result<()>`
- Create `.rahzom/` if doesn't exist
- Use JSON for MVP (human-readable, debuggable)

#### 2.3 Implement deleted files registry
- Add to registry on delete
- Cleanup entries older than 90 days on load
- Configurable retention period

#### 2.4 Write tests
- Save and load roundtrip
- Load non-existent (fresh start)
- Deleted files cleanup
- Corrupted file handling

### Definition of Done
- [x] Can save scan result to `.rahzom/state.json`
- [x] Can load previous state
- [x] Deleted files tracked with TTL
- [x] Tests pass

---

## Stage 3: Differ

**Goal**: Compare two scan results (or scan + metadata) and produce list of required actions.

### Tasks

#### 3.1 Define action types
```rust
// src/sync/differ.rs

pub enum SyncAction {
    CopyToRight { path: PathBuf, size: u64 },
    CopyToLeft { path: PathBuf, size: u64 },
    DeleteRight { path: PathBuf },
    DeleteLeft { path: PathBuf },
    Conflict { 
        path: PathBuf, 
        reason: ConflictReason,
        left: Option<FileInfo>,
        right: Option<FileInfo>,
    },
    CreateDirRight { path: PathBuf },
    CreateDirLeft { path: PathBuf },
    Skip { path: PathBuf, reason: String },
}

pub enum ConflictReason {
    BothModified,
    ModifiedAndDeleted,
    // ...
}

pub struct DiffResult {
    pub actions: Vec<SyncAction>,
    pub total_bytes_to_transfer: u64,
    pub files_to_copy: usize,
    pub files_to_delete: usize,
    pub conflicts: usize,
}
```

#### 3.2 Implement diff algorithm
```rust
pub fn diff(
    left_scan: &ScanResult,
    right_scan: &ScanResult,
    left_meta: &SyncMetadata,
    right_meta: &SyncMetadata,
) -> DiffResult
```

Logic:
1. Build maps: path → FileEntry for both sides
2. For each file on left:
   - Not on right + not in right's deleted → CopyToRight
   - Not on right + in right's deleted (matching hash) → Conflict (deleted vs exists)
   - On right + same state → Skip
   - On right + left changed + right unchanged → CopyToRight
   - On right + both changed → Conflict
3. For each file on right not on left:
   - Similar logic, mirror direction
4. Handle deleted files registry
5. FAT32 mtime tolerance (±2 sec)

#### 3.3 Write tests
- New file on left → copy right
- New file on right → copy left
- Modified left, unchanged right → copy right
- Both modified → conflict
- Deleted left, unchanged right → delete right
- Deleted left, modified right → conflict
- Same file both sides → skip
- FAT32 time tolerance
- First sync (no metadata)

### Definition of Done
- [x] Diff produces correct actions for all scenarios
- [x] Conflicts properly identified
- [x] Tests cover all change detection matrix
- [x] Performance OK for 10k files (< 1 sec)

---

## Stage 4: Executor

**Goal**: Execute synchronization actions (copy, delete, create dir).

### Tasks

#### 4.1 Define executor interface
```rust
// src/sync/executor.rs

pub struct ExecutorConfig {
    pub backup_enabled: bool,
    pub backup_versions: usize,
    pub soft_delete: bool,
}

pub struct ExecutionResult {
    pub completed: Vec<CompletedAction>,
    pub failed: Vec<FailedAction>,
    pub skipped: Vec<SkippedAction>,
}

pub trait ProgressCallback {
    fn on_progress(&mut self, current: usize, total: usize, current_file: &Path);
    fn on_file_complete(&mut self, action: &SyncAction, success: bool);
}
```

#### 4.2 Implement operations
- **Copy file**: 
  - Create parent dirs if needed
  - Copy content
  - Preserve mtime
  - Verify after copy (size check)
  
- **Delete file**:
  - If soft_delete: move to `.rahzom/_trash/{filename}.{timestamp}`
  - Else: delete
  
- **Backup before overwrite**:
  - Copy to `.rahzom/_backup/{filename}.{timestamp}`
  - Rotate old backups (keep N versions)

- **Create directory**:
  - Create with parents

#### 4.3 Implement execution order
1. Create all directories (sorted by depth, parents first)
2. Copy/update all files
3. Delete files
4. Delete empty directories (sorted by depth, children first)

#### 4.4 Pre-copy verification
Before each copy:
- Check source still exists
- Check source size/mtime unchanged since analyze
- If changed → skip, add to "changed during sync" list

#### 4.5 Write tests
- Copy single file
- Copy preserves mtime
- Delete with soft delete
- Backup before overwrite
- Backup rotation
- Execution order (dirs before files)
- File changed during sync → skip

### Definition of Done
- [x] Can execute full sync between two test folders
- [x] Backups created correctly
- [x] Soft delete works
- [x] mtime preserved
- [x] Tests pass

---

## Stage 5: Project Configuration

**Goal**: Manage projects (create, load, save, list).

### Tasks

#### 5.1 Define project structure
```rust
// src/config/project.rs

pub struct Project {
    pub name: String,
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    pub settings: ProjectSettings,
}

pub struct ProjectSettings {
    pub verify_hash: bool,
    pub backup_versions: usize,
    pub deleted_retention_days: u32,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            verify_hash: false,
            backup_versions: 5,
            deleted_retention_days: 90,
        }
    }
}
```

#### 5.2 Implement project manager
```rust
pub struct ProjectManager {
    config_dir: PathBuf,  // ~/.rahzom/
}

impl ProjectManager {
    pub fn list_projects(&self) -> Result<Vec<String>>
    pub fn load_project(&self, name: &str) -> Result<Project>
    pub fn save_project(&self, project: &Project) -> Result<()>
    pub fn delete_project(&self, name: &str) -> Result<()>
    pub fn project_exists(&self, name: &str) -> bool
}
```

#### 5.3 Config directory structure
```
~/.rahzom/
├── config.toml           # Global settings (future)
└── projects/
    ├── docs.toml
    └── med.toml
```

#### 5.4 Write tests
- Create project
- Load project
- List projects
- Save and reload
- Invalid project name handling

### Definition of Done
- [x] Projects persist to disk
- [x] Can list, create, load, delete projects
- [x] Tests pass

---

## Stage 6: TUI Shell

**Goal**: Basic TUI application that starts, shows something, handles keyboard, exits cleanly.

### Tasks

#### 6.1 Setup Ratatui boilerplate
- Initialize terminal
- Setup panic handler (restore terminal on crash)
- Event loop
- Clean exit on 'q'

#### 6.2 Basic app state
```rust
// src/app.rs

pub enum Screen {
    ProjectList,
    ProjectView,
    Analyzing,
    Preview,
    Syncing,
}

pub struct App {
    pub screen: Screen,
    pub should_quit: bool,
    // ...
}
```

#### 6.3 Minimal rendering
- Show "rahzom v0.1.0" header
- Show "Press Q to quit"
- Handle resize

#### 6.4 Mouse support setup
- Enable mouse capture
- Log mouse events (for debugging)

### Definition of Done
- [x] App starts in terminal
- [x] Shows header
- [x] Q exits cleanly
- [x] No crashes on resize
- [x] Mouse events captured

---

## Stage 7: TUI Project List

**Goal**: Startup screen showing list of projects, ability to select or create.

### Tasks

#### 7.1 Project list widget
- List all projects from ProjectManager
- Arrow keys to navigate
- Enter to select
- Visual highlight of selected item

#### 7.2 New project flow
- [N] key opens "new project" dialog
- Text input for project name
- Text input for left path (+ browse button placeholder)
- Text input for right path (+ browse button placeholder)
- Validation (paths exist or offer to create)
- Save project

#### 7.3 Navigation
- Selected project → go to Project View screen
- Escape from sub-dialogs

#### 7.4 Mouse support
- Click to select project
- Double-click to open

### Definition of Done
- [x] Shows list of existing projects
- [x] Can navigate with keyboard
- [x] Can select project (goes to next screen placeholder)
- [x] Can create new project via dialog
- [x] Mouse selection works

---

## Stage 8: TUI Analyze & Preview

**Goal**: Tree view showing files and proposed sync actions.

### Tasks

#### 8.1 Integrate core with UI
- On entering ProjectView: show project info
- [A] triggers analyze (calls Scanner + Differ)
- Loading indicator during analyze

#### 8.2 Tree view widget
- Show files in tree structure
- Expand/collapse folders (Enter or arrow)
- Columns: Name, Left info, Action, Right info
- Unicode arrows for actions (→, ←, ↔, ✕)
- Color coding (green=copy, red=delete, yellow=conflict)

#### 8.3 Filters
- [F] cycles filters: All → Changes → Conflicts
- Show filter state in UI
- Filter affects visible items only (not actions)

#### 8.4 Action modification
- Left/Right arrow keys change action direction
- [D] sets skip
- [Delete] sets delete
- Visual feedback when action changed from default

#### 8.5 Bulk operations
- [Space] toggles selection
- Shift+arrow for range select
- Apply action to all selected

#### 8.6 Summary bar
- Show totals: X files to copy, Y to delete, Z conflicts
- Show total bytes to transfer

### Definition of Done
- [x] Analyze runs and shows results
- [x] Tree view renders correctly
- [x] Can expand/collapse folders
- [x] Can change actions on individual files
- [x] Filters work
- [x] Summary shows correct totals

---

## Stage 8.a: Handle Missing Directories (fix)

**Goal**: When user runs Analyze, handle non-existent paths gracefully.

### Problem
Scanner fails with "Failed to canonicalize path" when directory doesn't exist instead of offering to create it.

### Tasks

#### 8.a.1 Check paths before analyze
- Before calling `scan()`, check if both paths exist
- If both missing → show error "At least one directory must exist"
- If one missing → show dialog "Directory X doesn't exist. Create it? [Y/N]"

#### 8.a.2 Add CreateDirConfirm dialog
- Y/Enter: create directory with `fs::create_dir_all()`, then re-run analyze
- N/Esc: close dialog, return to ProjectView

#### 8.a.3 Tests
- Both paths missing → error dialog
- Left path missing → create confirm dialog
- Right path missing → create confirm dialog
- Create on confirm → analyze runs

### Definition of Done
- [x] Both missing → error message
- [x] One missing → offer to create
- [x] After creation → analyze runs automatically
- [x] Tests pass

---

## Stage 9: TUI Sync Execution

**Goal**: Execute sync with progress display.

### Tasks

#### 9.1 Sync confirmation
- [S] from preview shows confirmation dialog
- Show summary: "Copy X files, delete Y files, transfer Z MB"
- [Enter] to confirm, [Esc] to cancel

#### 9.2 Progress screen
- Progress bar (files processed / total)
- Progress bar (bytes transferred / total)
- Current file being processed
- Elapsed time
- Estimated remaining time

#### 9.3 Execution integration
- Run Executor with progress callback
- Update UI on each file
- Handle interruption (Esc during sync → confirm cancel)

#### 9.4 Completion screen
- Summary: completed, failed, skipped
- List of errors if any
- [Enter] to return to project view

#### 9.5 "Changed during sync" handling
- If files changed → show list after sync
- Offer to re-analyze

### Definition of Done
- [x] Sync executes with visual progress
- [x] Can cancel sync (with confirmation)
- [x] Shows completion summary
- [x] Errors displayed clearly
- [x] Full cycle works: analyze → preview → modify → sync

---

## Stage 9b: Refactoring

**Goal**: Split app.rs (2,725 lines) into manageable modules. No functional changes.

**Version**: 0.10.0 → 0.10.1 (patch)

### Target Structure

```
src/
├── app.rs              # ~600 lines: App struct, run(), core methods
├── app/
│   ├── mod.rs          # Re-exports
│   ├── state.rs        # Screen, Dialog enums, PreviewState, etc.
│   └── handlers.rs     # Keyboard/mouse event handlers
└── ui/
    ├── mod.rs          # Re-exports
    ├── dialogs.rs      # Dialog rendering
    ├── screens.rs      # Screen rendering
    ├── sync_ui.rs      # Sync progress/complete screens
    └── widgets.rs      # format_bytes, centered_rect, helpers
```

### Tasks

#### 9b.1 Extract UI widgets
- `format_bytes()`, `format_duration()`, `centered_rect()` → `ui/widgets.rs`

#### 9b.2 Extract dialogs
- All `render_*_dialog()` functions → `ui/dialogs.rs`
- Dialog structs: `NewProjectDialog`, `SyncConfirmDialog`

#### 9b.3 Extract screen rendering
- `render_project_list()`, `render_preview()`, etc. → `ui/screens.rs`
- `render_syncing()`, `render_sync_complete()` → `ui/sync_ui.rs`

#### 9b.4 Extract state types
- Enums: `Screen`, `Dialog`, `DialogField`, `PreviewFilter` → `app/state.rs`
- Structs: `PreviewState`, `SyncingState`, `SyncCompleteState` → `app/state.rs`

#### 9b.5 Extract event handlers
- All `handle_key_*()` methods → `app/handlers.rs`

#### 9b.6 Eliminate code duplication
- FAT32 tolerance (duplicated in differ.rs and executor.rs) → `sync/utils.rs`

#### 9b.7 Update documentation
- Update project structure in: `docs/ARCHITECTURE.md`, `README.md`, `CLAUDE.md`

### Definition of Done
- [x] app.rs reduced from 2,725 to ~1,073 lines (app/mod.rs)
- [x] All 73 tests pass
- [x] `cargo clippy` clean
- [x] No functional changes (same behavior)
- [x] Documentation updated

### Result
- `app/mod.rs`: 1,073 lines (was 2,725 in app.rs — 61% reduction)
- `app/state.rs`: 352 lines
- `app/handlers.rs`: 546 lines
- `ui/widgets.rs`: 92 lines
- `ui/dialogs.rs`: 271 lines
- `ui/screens.rs`: 296 lines
- `ui/sync_ui.rs`: 194 lines
- `sync/utils.rs`: 38 lines

---

## Stage 10: Filtering & Exclusions

**Goal**: Pattern-based file exclusions.

### Tasks

#### 10.1 Exclusion rules format
```
# .rahzom/exclusions.conf
*.tmp
*.temp
.DS_Store
Thumbs.db
node_modules/
.git/
__pycache__/
```

#### 10.2 Pattern matching
- Glob patterns (use `glob` or `globset` crate)
- Directory patterns (trailing `/`)
- Apply during scan (skip excluded)

#### 10.3 Default exclusions
- Create with sensible defaults on first run
- User can edit file directly

#### 10.4 UI for exclusions (minimal)
- Show in project config screen
- Text area to edit patterns
- Or just show path to file for manual editing

#### 10.5 Sync exclusions between sides
- `.rahzom/exclusions.conf` should be same on both sides
- On analyze: if different, show warning
- Option to sync exclusions file

### Definition of Done
- [x] Excluded files not shown in scan
- [x] Default exclusions created
- [x] Can edit exclusions
- [x] Exclusions sync between sides

---

## Stage 10b: Simplify Exclusions System

**Goal**: Simplify exclusions based on practical feedback.

**Version**: 0.11.0 → 0.11.1 (patch)

### Changes from Stage 10

| Before | After |
|--------|-------|
| `.rahzom/exclusions.conf` (hidden) | `.rahzomignore` in root (visible, syncs naturally) |
| Auto-created on first analyze | Opt-in only |
| Built-in default exclusions | Only `.rahzom/` hardcoded |
| Editor integration (L/R keys) | Manual editing |
| Copy between sides (C/V keys) | File syncs naturally |

### Tasks

#### 10b.1 Move exclusions file
- Change location from `.rahzom/exclusions.conf` to `.rahzomignore` in root
- File syncs naturally between sides like any other file

#### 10b.2 Make opt-in
- No automatic creation of exclusions file
- Add "Create template" command (T key in dialog)
- Template includes common patterns + syntax comments

#### 10b.3 Remove built-in defaults
- Only `.rahzom/` folder is hardcoded (metadata)
- All other exclusions come from `.rahzomignore` file

#### 10b.4 Simplify UI
- Remove editor integration (was L/R keys)
- Remove copy between sides (was C/V keys)
- Show file existence status and pattern counts
- Add "Create template" option (T key)

### Definition of Done
- [x] `.rahzomignore` in root instead of `.rahzom/exclusions.conf`
- [x] No auto-creation (opt-in)
- [x] Create template command works
- [x] UI simplified
- [x] Tests pass

---

## Stage 10c: Fix Direction Change for One-Sided Files

**Goal**: Fix bug where changing direction on one-sided files causes error.

**Version**: 0.11.1 → 0.11.2 (patch)

### Problem

When file exists only on one side and user changes direction:
- Original action: `CopyToRight` (file exists on left)
- User presses LEFT arrow expecting to delete
- Code changed to `CopyToLeft` which fails (no source on right)

### Fix

Arrows now mean "which side is source of truth":

| File Location | RIGHT arrow (→) | LEFT arrow (←) |
|---------------|-----------------|----------------|
| Only on LEFT | CopyToRight | DeleteLeft |
| Only on RIGHT | DeleteRight | CopyToLeft |
| Both sides | CopyToRight | CopyToLeft |

### Tasks

#### 10c.1 Add new UserAction variants
- Add `DeleteLeft { path }` and `DeleteRight { path }` to UserAction enum
- Update `path()`, `to_sync_action()`, `summary()` methods

#### 10c.2 Fix direction change logic
- `change_action_to_left()`: if file doesn't exist on right → DeleteLeft
- `change_action_to_right()`: if file doesn't exist on left → DeleteRight

#### 10c.3 Update rendering
- Add rendering for `UserAction::DeleteLeft` → "←✕*"
- Add rendering for `UserAction::DeleteRight` → "✕→*"

### Definition of Done
- [x] New UserAction variants added
- [x] Direction change creates delete action when appropriate
- [x] Delete actions rendered correctly
- [x] Tests pass

---

## Stage 11: Error Handling & Edge Cases

**Goal**: Handle real-world edge cases gracefully.

### Tasks

#### 11.1 Locked files (Windows)
- Detect "file in use" error
- Show dialog: Retry / Skip / Cancel
- Retry checks file unchanged

#### 11.2 Permission errors
- Detect permission denied
- Show dialog: Skip / Cancel
- Log error

#### 11.3 Disk space check
- Before sync: estimate required space
- Compare with available space
- Warn if insufficient (but allow proceed)

#### 11.4 Long paths (Windows)
- Ensure `\\?\` prefix used throughout
- Test with path > 260 chars

#### 11.5 Filename case handling
- Store original case in metadata
- Compare case-insensitively
- Detect case-only conflicts on Linux

#### 11.6 Symlinks
- Detect during scan
- Skip with warning in log
- Show skipped symlinks in UI (optional)

#### 11.7 FAT32 time tolerance
- Detect filesystem type (if possible) or always apply tolerance
- Use ±2 second window for mtime comparison

### Definition of Done
- [x] Locked file handled gracefully
- [x] Permission errors handled
- [x] Long paths work on Windows
- [x] Symlinks skipped safely
- [x] FAT32 doesn't cause false changes

---

## Stage 12: Polish & Integration Tests

**Goal**: End-to-end testing, documentation, release preparation.

### Tasks

#### 12.1 Integration tests
- Full cycle: create project → analyze → sync → verify
- Test with generated complex folder structure
- Test conflict resolution flows
- Test error recovery

#### 12.2 Cross-platform testing
- Test on Windows (especially long paths)
- Test on Linux
- Test on macOS (if available)

#### 12.3 Performance testing
- Scan 50k files, measure time
- Sync 1000 files, measure time
- Profile and optimize if needed

#### 12.4 README
- Installation instructions
- Quick start guide
- Screenshots
- Known limitations

#### 12.5 Release build
- `cargo build --release`
- Test release binary
- Strip symbols for smaller size (optional)

#### 12.6 GitHub release (optional)
- Tag version
- Build binaries for all platforms via CI
- Create release with binaries

### Definition of Done
- [x] Integration tests pass
- [x] Tested on target platforms
- [x] README complete
- [x] Release binary works
- [x] Performance acceptable

---

## Appendix: Testing Strategy

### Unit Tests
- In each module (`scanner.rs`, `differ.rs`, etc.)
- Use `#[cfg(test)]` module
- Use `tempfile` crate for filesystem tests
- Focus on happy path + key edge cases

### Integration Tests
- In `tests/` directory
- Full scenarios with test data generator
- Slower, run separately

### Test Data Generator Specs
```rust
pub struct TreeSpec {
    pub files: Vec<FileSpec>,
}

pub struct FileSpec {
    pub path: &str,           // Relative path
    pub content: Content,     // Fixed, Random(size), or FromFile
    pub mtime: Option<DateTime<Utc>>,  // None = now
}

pub enum Content {
    Fixed(&'static str),
    Random(usize),            // Random bytes of given size
    Empty,
}

// Usage:
let spec = TreeSpec {
    files: vec![
        FileSpec::new("docs/readme.txt").content("Hello"),
        FileSpec::new("docs/report.pdf").random(10000),
        FileSpec::new("empty/").dir(),
    ],
};
let temp = create_test_tree(&spec);
```

### Manual Testing Checklist
Before each "done" checkpoint:
- [ ] Run on real folder (small, safe one)
- [ ] Try canceling mid-operation
- [ ] Try with USB drive
- [ ] Check no data loss on source

---

## Appendix: Rust TDD Quick Reference

For someone new to Rust testing:

### Running tests
```bash
cargo test                    # All tests
cargo test scanner            # Tests with "scanner" in name
cargo test -- --nocapture     # Show println! output
cargo test -- --test-threads=1  # Sequential (for filesystem tests)
```

### Test structure
```rust
// In src/sync/scanner.rs

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_scan_empty_dir() {
        let temp = TempDir::new().unwrap();
        let result = Scanner::scan(temp.path()).unwrap();
        assert!(result.entries.is_empty());
    }

    #[test]
    fn test_scan_with_files() {
        // ...
    }
}
```

### Assertions
```rust
assert!(condition);
assert_eq!(actual, expected);
assert_ne!(actual, expected);
assert!(result.is_ok());
assert!(result.is_err());
```

### Test utilities location
Put shared test helpers in `tests/common/mod.rs`, import with:
```rust
mod common;
use common::create_test_tree;
```
