**rahzom** (from Ukrainian "разом" = together) is a lightweight cross-platform folder synchronization utility. Alternative to GoodSync with focus on simplicity and portability.

- Language: Rust
- UI: TUI (Ratatui + crossterm)
- License: MIT

## Commands

```bash
# Build
cargo build                      # Debug build
cargo build --release            # Release build

# Run
cargo run                        # Run debug build

# Tests
cargo test                       # All tests
cargo test scanner               # Tests with "scanner" in name
cargo test -- --nocapture        # Show println! output
cargo test -- --test-threads=1   # Sequential (for filesystem tests)

# Check & Lint
cargo check                      # Fast compile check
cargo clippy                     # Lints
cargo fmt                        # Format code
```

## Architecture

```
User → TUI (ui/) → App State (app/) → Sync Logic (sync/) → Filesystem
                                             ↓
                                    Config (config/) → ~/.rahzom/
                                             ↓
                                    Metadata (.rahzom/ in sync folders)
```

### Project Structure

```
src/
├── main.rs           # Entry point, TUI initialization
├── lib.rs            # Re-exports for testing
├── app/              # Application module
│   ├── mod.rs        # App struct, business logic, rendering
│   ├── state.rs      # Screen, Dialog, PreviewState, etc.
│   └── handlers.rs   # Event handling (keyboard, mouse)
├── sync/             # Core synchronization logic
│   ├── mod.rs
│   ├── scanner.rs    # Filesystem scanning
│   ├── differ.rs     # Compare states, generate actions
│   ├── executor.rs   # Execute copy/delete operations
│   ├── exclusions.rs # File exclusion patterns
│   ├── metadata.rs   # .rahzom/ folder management
│   └── utils.rs      # Shared utilities (FAT32 tolerance)
├── config/           # Project configuration
│   ├── mod.rs
│   └── project.rs    # Project settings (~/.rahzom/)
└── ui/               # TUI components
    ├── mod.rs
    ├── widgets.rs    # Helpers: format_bytes, centered_rect
    ├── dialogs.rs    # Dialog rendering functions
    ├── screens.rs    # Main screen rendering
    └── sync_ui.rs    # Sync progress screens

tests/
├── common/
│   └── mod.rs        # Test utilities, data generators
└── test_common.rs
```

### Key Concepts

- **Project**: A pair of folders to sync, stored in `~/.rahzom/projects/`
- **Metadata**: File state stored in `.rahzom/` inside each synced folder
- **Sync flow**: Analyze → Preview (user can modify actions) → Sync

## File Exclusions

Exclusion patterns are stored in `.rahzomignore` file in the root of each synced folder.
The file syncs naturally between sides like any other file.

### Creating Exclusions

Exclusions are opt-in. To create a `.rahzomignore` file:
1. Press `E` in Preview screen to open exclusions dialog
2. Press `T` to create template with common patterns on both sides
3. Or create `.rahzomignore` manually in your text editor

The `.rahzom/` metadata folder is always excluded automatically.

### Pattern Syntax

| Pattern | Description | Example |
|---------|-------------|---------|
| `*` | Matches any characters except `/` | `*.tmp` matches `file.tmp` |
| `**` | Matches any characters including `/` | `**/*.log` matches `a/b/c/app.log` |
| `?` | Matches single character | `file?.txt` matches `file1.txt` |
| `[abc]` | Matches character class | `[0-9].txt` matches `1.txt` |
| `{a,b}` | Matches alternatives | `*.{tmp,temp}` matches both |
| `dir/` | Directory pattern (trailing `/`) | `node_modules/` excludes dir and contents |

### Template Exclusions

The template includes common patterns:
- Temporary: `*.tmp`, `*.temp`, `~*`, `*~`
- OS files: `.DS_Store`, `Thumbs.db`, `desktop.ini`
- VCS: `.git/`, `.svn/`, `.hg/`
- Development: `node_modules/`, `__pycache__/`, `target/`, `build/`
- IDE: `.idea/`, `.vscode/`, `*.swp`

## Dependencies

Key crates used:
- `ratatui` + `crossterm` — TUI framework
- `serde` + `serde_json` — Serialization
- `walkdir` — Directory traversal
- `chrono` — Date/time handling
- `anyhow` — Error handling
- `sha2` — Hashing (optional file verification)
- `globset` — File exclusion pattern matching
- `dirs` — Platform-specific directories (~/.rahzom)
- `tempfile` — Temporary directories for tests

## Documentation

- `docs/ARCHITECTURE.md` — High-level design decisions
- `docs/REQUIREMENTS.md` — Detailed functional requirements
- `docs/PLAN.md` — Implementation stages and tasks
- `docs/DEVIATIONS.md` — Changes from original plan (created as needed)

## Testing

Tests are alongside code in `#[cfg(test)]` modules. Integration tests in `tests/`.

Test data generator creates temporary folder structures:
```rust
// In tests, use common::create_test_tree()
let temp = create_test_tree(&TreeSpec {
    files: vec![
        FileSpec::new("docs/readme.txt").content("Hello"),
        FileSpec::new("data/").dir(),
    ],
});
```

Always use `tempfile::TempDir` for filesystem tests — auto-cleanup on drop.
