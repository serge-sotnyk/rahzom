# rahzom — Architecture Document

> Name from Ukrainian "разом" (razom = together) — files are always together, synchronized.

## Project Overview

A lightweight cross-platform utility for bidirectional folder synchronization between computers and removable drives. An alternative to GoodSync with focus on simplicity and portability.

## Technology Stack

| Component | Technology | Rationale |
|-----------|------------|-----------|
| Language | **Rust** | Compiles to native binary, no runtime dependencies, excellent filesystem support, cross-platform |
| UI | **TUI (Ratatui + crossterm)** | Modern terminal interface with mouse support, small size, works everywhere |
| Future GUI | **Tauri** | Same Rust backend, can be added later without rewriting logic |

## Characteristics

- **Binary size**: ~5-10 MB
- **Platforms**: Windows, macOS, Linux
- **User dependencies**: none
- **Installer**: not required (portable)
- **Framework licenses**: MIT (commercial use permitted)

## Key Features (MVP)

1. **Bidirectional synchronization** of folders between two sources
2. **Change preview** before applying — user sees action list and can modify/cancel individual operations
3. **Conflict resolution** via sidecar metadata (hidden folder with file state information)
4. **Mouse support** in TUI for convenient navigation
5. **Run from USB drive** — fully portable application

## Technical Requirements

- Windows long path support (>260 characters)
- Scale: tens of thousands of files
- Metadata storage in shadow folder alongside synchronized data

## User Workflow

```
1. Insert USB drive
2. Launch the application (manually)
3. [First run] Add folder pairs for synchronization
4. Click "Analyze" — program shows action plan
5. [Optional] Modify individual actions (choose different file version, cancel deletion)
6. Click "Sync" — execute synchronization
```

## Project Structure (Preliminary)

```
rahzom/
├── src/
│   ├── main.rs           # Entry point, TUI initialization
│   ├── app.rs            # Application state, main loop
│   ├── ui/               # Interface components
│   │   ├── mod.rs
│   │   ├── file_list.rs  # File/change list display
│   │   ├── preview.rs    # Preview screen before sync
│   │   └── config.rs     # Folder pair configuration
│   ├── sync/             # Synchronization logic
│   │   ├── mod.rs
│   │   ├── scanner.rs    # Filesystem scanning
│   │   ├── differ.rs     # State comparison, action determination
│   │   ├── executor.rs   # Copy/delete operation execution
│   │   └── metadata.rs   # Sidecar metadata handling
│   └── config/           # Application configuration
│       ├── mod.rs
│       └── pairs.rs      # Sync pair storage
├── Cargo.toml
├── README.md
└── .github/
    └── workflows/
        └── release.yml   # CI/CD for cross-platform builds
```

## Dependencies (Cargo.toml)

```toml
[dependencies]
ratatui = "0.29"          # TUI framework
crossterm = "0.28"        # Terminal backend (cross-platform)
serde = { version = "1", features = ["derive"] }  # Config serialization
serde_json = "1"          # JSON for metadata
walkdir = "2"             # Recursive directory traversal
chrono = "0.4"            # Date/time handling
anyhow = "1"              # Convenient error handling
```

## Future Extensions (Post-MVP)

- [ ] GUI version with Tauri
- [ ] Cloud synchronization (Google Drive, Dropbox)
- [ ] Encryption
- [ ] Automatic sync on device connection
- [ ] Code signing for Windows/macOS

## Deliberate Design Decisions

1. **TUI over GUI** — faster development, smaller size, universal compatibility. GUI can be added later.
2. **No installer** — portability is more important than system integration.
3. **Rust over Python** — experiment in AI-assisted development in an unfamiliar language + native binary without dependencies.
4. **Sidecar metadata** — enables smart conflict resolution without a central database.

## Context for AI Assistant (Claude Code)

When working on this project:
- Ratatui documentation available via Context7 (`/websites/rs_ratatui`, 7198 snippets, Trust Score 9.7)
- Tauri documentation also available (`/websites/rs_tauri_2_9_5`, 16899 snippets, Trust Score 9.7)
- Reference project with mouse support: https://github.com/ricott1/rebels-in-the-sky
