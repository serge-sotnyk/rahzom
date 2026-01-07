# Refactoring Plan for rahzom

> Analysis performed: 2025-01-07
> Current version: Stage 8.a completed
> Status: Ready for future refactoring session

---

## Code Statistics

| File | Lines | Test Lines | Test % |
|------|-------|------------|--------|
| **app.rs** | 2,668 | 299 | 11% |
| executor.rs | 816 | 331 | 41% |
| differ.rs | 752 | 338 | 45% |
| config/project.rs | 413 | 161 | 39% |
| sync/metadata.rs | 365 | 183 | 50% |
| sync/scanner.rs | 334 | 138 | 41% |
| Other files | ~30 | 0 | - |
| **TOTAL** | 5,386 | ~1,250 | 23% |

---

## Key Findings

### 1. Tests in Same Files - Standard Rust Practice

Unit tests inside `#[cfg(test)]` modules in source files is **idiomatic Rust**:
- Allows testing private functions
- Keeps tests close to code
- All major Rust projects do this

**No change needed** - this is correct.

### 2. Main Problem: app.rs = 50% of All Code

**app.rs (2,668 lines)** is a classic "god file" containing:

- 7 screen types (Screen enum)
- 7 dialog types (Dialog enum)
- App struct with ~35 fields
- 80+ methods
- 17+ render functions
- 10+ keyboard handlers
- Business logic (analyze, sync, metadata)
- All UI state management

**Why this is a problem:**
- Any UI change requires loading 2,668 lines into context
- AI assistants fill context window quickly
- Hard to test UI logic separately
- Mixing concerns: state, rendering, event handling

### 3. sync/ and config/ Modules - Well Organized

These modules have:
- Clear separation of concerns
- Good test coverage (40-50%)
- Reasonable file sizes (300-800 lines)
- No circular dependencies
- Clean data flow: scanner → differ → executor

**No change needed** for these modules.

### 4. Dependency Graph (No Circular Dependencies)

```
main.rs
  └─→ app.rs
        ├─→ config::project
        ├─→ sync::scanner
        ├─→ sync::differ ──→ sync::metadata, sync::scanner
        ├─→ sync::executor ──→ sync::differ
        └─→ sync::metadata
```

---

## Refactoring Recommendations

### Phase 1: Split app.rs into Modules (HIGH PRIORITY)

Proposed structure:

```
src/
├── app.rs              # ~600 lines: App struct, run(), core methods
├── app/
│   ├── mod.rs          # Re-exports
│   ├── state.rs        # Screen, Dialog enums, helper structs
│   └── handlers.rs     # Keyboard/mouse event handling
└── ui/
    ├── mod.rs          # Re-exports
    ├── dialogs.rs      # ~400 lines: all dialogs
    ├── preview.rs      # ~300 lines: preview screen, file tree
    ├── progress.rs     # ~200 lines: progress screen
    ├── project_list.rs # ~200 lines: project list screen
    └── widgets.rs      # ~100 lines: format_bytes(), centered_rect(), etc.
```

**Expected result:**
- app.rs: 2,668 → ~600 lines
- Each UI component: 100-400 lines
- AI can work with individual files without loading entire UI

### Phase 2: Eliminate Code Duplication (MEDIUM PRIORITY)

#### 2.1 FAT32 Tolerance - Duplicated in 3 Places

```rust
// differ.rs:381-382
let tolerance = Duration::seconds(2);
// differ.rs:400-402
let tolerance = Duration::seconds(2);
// executor.rs:300-302
let tolerance = Duration::seconds(2);
```

**Solution:** Extract to `sync/utils.rs`:
```rust
pub const FAT32_TOLERANCE: Duration = Duration::seconds(2);

pub fn times_equal_with_tolerance(t1: DateTime<Utc>, t2: DateTime<Utc>) -> bool {
    (t1 - t2).abs() <= FAT32_TOLERANCE
}
```

#### 2.2 Dialog Field Styling - Repeated 3+ Times

In `render_new_project_dialog()`:
```rust
let name_style = if dialog.focused_field == DialogField::Name {
    Style::default().fg(Color::Yellow)
} else {
    Style::default().fg(Color::DarkGray)
};
// Same pattern for left_path_style, right_path_style...
```

**Solution:** Extract helper:
```rust
fn field_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
```

#### 2.3 FileState Construction in save_sync_metadata()

Lines 1280-1322: repeated FileState construction in match arms.

**Solution:** Extract method:
```rust
fn file_state_from_action(action: &SyncAction, now: DateTime<Utc>) -> Option<FileState> { ... }
```

### Phase 3: Improve UI Testability (LOW PRIORITY)

- Extract pure functions from render methods
- Add unit tests for UI logic (filtering, sorting, etc.)

---

## Refactoring Order

Recommended step-by-step approach:

### Step 1: Extract UI Widgets (Safe, Non-Breaking)
- `format_bytes()`, `format_duration()`, `centered_rect()` → `ui/widgets.rs`
- Run tests after each step

### Step 2: Extract Dialogs
- All `render_*_dialog()` functions → `ui/dialogs.rs`
- NewProjectDialog, SyncConfirmDialog structs → same file

### Step 3: Extract Screens
- Preview rendering → `ui/preview.rs`
- Progress rendering → `ui/progress.rs`
- Project list rendering → `ui/project_list.rs`

### Step 4: Split App State and Handlers
- Enums and helper structs → `app/state.rs`
- Event handlers → `app/handlers.rs`

### Step 5: Eliminate Duplication
- FAT32 tolerance → `sync/utils.rs`
- Dialog styling helpers
- FileState construction helpers

---

## Large Functions to Refactor

| Function | Lines | Location | Issue |
|----------|-------|----------|-------|
| `render_action_item()` | 57 | app.rs | 12 match arms with similar pattern |
| `render_new_project_dialog()` | ~86 | app.rs | Repeated field styling |
| `render_sync_complete()` | ~112 | app.rs | Complex state handling |
| `run_analyze()` | 57 | app.rs | 4 validation branches |
| `save_sync_metadata()` | 69 | app.rs | Repeated FileState construction |
| `execute_next_sync_action()` | 64 | app.rs | Deep pattern matching |
| `render_preview()` | ~68 | app.rs | Mixed concerns |

---

## What NOT to Change

1. **Tests in files** - keep as is (idiomatic Rust)
2. **sync/ structure** - already well organized
3. **config/ structure** - works fine
4. **Data flow architecture** - scanner → differ → executor is excellent
5. **Error handling with anyhow** - correct approach

---

## Benefits of Refactoring

1. **AI Context Efficiency**: Smaller files = less context needed per task
2. **Easier Testing**: Isolated components can be unit tested
3. **Maintainability**: Changes to dialogs don't require understanding all of app.rs
4. **Parallel Development**: Different UI components can be worked on independently
5. **Code Reuse**: Extracted widgets can be reused

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Breaking existing functionality | Run full test suite after each step |
| Introducing import cycles | Follow dependency direction (ui → app → sync) |
| Merge conflicts with pending changes | Complete current bug fixes first |
| Scope creep | Stick to extraction only, no new features |

---

## Estimated Effort

- Phase 1 (Split app.rs): 1-2 sessions
- Phase 2 (Eliminate duplication): 0.5 session
- Phase 3 (Improve testability): Optional, 0.5 session

**Total: 2-3 focused sessions**

---

## Session Checklist

Before starting refactoring:
- [ ] All current bugs fixed
- [ ] All tests passing
- [ ] Clean git status
- [ ] Backup/branch created

After each step:
- [ ] `cargo build` succeeds
- [ ] `cargo test` passes
- [ ] `cargo clippy` clean
- [ ] Manual smoke test of UI
