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
User → TUI (ui/) → App State (app.rs) → Sync Logic (sync/) → Filesystem
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
├── app.rs            # Application state, main loop
├── sync/             # Core synchronization logic
│   ├── mod.rs
│   ├── scanner.rs    # Filesystem scanning
│   ├── differ.rs     # Compare states, generate actions
│   ├── executor.rs   # Execute copy/delete operations
│   └── metadata.rs   # .rahzom/ folder management
├── config/           # Project configuration
│   ├── mod.rs
│   └── project.rs    # Project settings (~/.rahzom/)
└── ui/               # TUI components
    ├── mod.rs
    ├── project_list.rs
    ├── preview.rs
    └── progress.rs

tests/
├── common/
│   └── mod.rs        # Test utilities, data generators
└── integration/
    └── mod.rs
```

### Key Concepts

- **Project**: A pair of folders to sync, stored in `~/.rahzom/projects/`
- **Metadata**: File state stored in `.rahzom/` inside each synced folder
- **Sync flow**: Analyze → Preview (user can modify actions) → Sync

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
