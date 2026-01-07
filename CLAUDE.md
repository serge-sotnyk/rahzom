**rahzom** (from Ukrainian "разом" = together) is a lightweight cross-platform folder synchronization utility. Alternative to GoodSync with focus on simplicity and portability.

- Language: Rust
- UI: TUI (Ratatui + crossterm)

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

## Code Style

- All comments and messages in code and documentation should be in English
- Don't write obvious comments in tutorial style. Only explanations for non-standard solutions
- Use idiomatic Rust: `Result<T, E>` for errors, `Option<T>` for optional values
- Use `anyhow` for error handling in application code
- Use `thiserror` if defining custom error types for libraries
- Prefer `impl Trait` over `dyn Trait` where possible
- Run `cargo fmt` before committing
- Run `cargo clippy` and fix warnings

## Dependencies

Key crates used:
- `ratatui` + `crossterm` — TUI framework
- `serde` + `serde_json` — Serialization
- `walkdir` — Directory traversal
- `chrono` — Date/time handling
- `anyhow` — Error handling
- `sha2` — Hashing (optional file verification)
- `dirs` — Platform-specific directories (~/.rahzom)
- `tempfile` — Temporary directories for tests

## Development Notes

- Use MCP context7 for up-to-date documentation
- Windows long paths (>260 chars) require `\\?\` prefix — use `dunce` crate or handle manually
- FAT32 has 2-second mtime precision — use tolerance in comparisons
- `.rahzom/` folder should be skipped during scanning
- Symlinks should be skipped with warning

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
