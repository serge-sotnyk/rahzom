# Windows Sandbox + MCP Testing Skills

Lightweight Windows TUI testing using Windows Sandbox + MCPControl. Alternative to Hyper-V VM — no ISO download, faster startup, full automation via MCP.

## Architecture

```
HOST (Windows)                    Windows Sandbox
┌──────────────┐                 ┌─────────────────────┐
│ Claude Code  │───SSE/HTTP─────►│ MCPControl Server   │
│              │                 │ (port 3232)         │
└──────────────┘                 │         │           │
                                 │         ▼           │
┌──────────────┐                 │ ┌─────────────────┐ │
│ C:\sandbox-  │◄───mapped──────►│ │ rahzom.exe TUI  │ │
│ share\       │    folder       │ └─────────────────┘ │
└──────────────┘                 └─────────────────────┘
```

## Skills to Create

| Skill | Purpose |
|-------|---------|
| `sandbox-wsb-init` | Enable Sandbox feature, generate .wsb config |
| `sandbox-wsb-testing` | Launch sandbox, connect MCP, run TUI tests |

## Files to Create

### sandbox-wsb-init

```
.claude/skills/sandbox-wsb-init/
├── SKILL.md              # Check Sandbox enabled, launch instructions
├── rahzom-sandbox.wsb    # Sandbox XML configuration
├── setup.ps1             # Runs inside sandbox: install deps, start MCP
└── connect.ps1           # Runs on host: wait for ready, show MCP URL
```

### sandbox-wsb-testing

```
.claude/skills/sandbox-wsb-testing/
├── SKILL.md              # MCP commands for TUI testing
└── presets.md            # Test data presets (reuse from sandbox-windows-testing)
```

## Key Components

### rahzom-sandbox.wsb

XML configuration:
- `<Networking>Enable</Networking>` — allows MCP connection from host
- `<MemoryInMB>4096</MemoryInMB>`
- MappedFolders: repo → `C:\rahzom`, share → `C:\share`
- LogonCommand: runs setup.ps1

### setup.ps1 (inside Sandbox)

Sequential steps:
1. Write sandbox IP to `C:\share\sandbox-ip.txt`
2. Download and install Rust (rustup-init.exe)
3. Download and install Node.js (MSI)
4. `npm install -g mcp-control`
5. Start MCPControl: `mcp-control --sse --port 3232`
6. Create test directories `C:\test\left`, `C:\test\right`
7. Build rahzom: `cargo build --release`
8. Write "READY" to `C:\share\status.txt`

### connect.ps1 (on Host)

1. Poll `C:\sandbox-share\status.txt` until exists
2. Read IP from `C:\sandbox-share\sandbox-ip.txt`
3. Print MCP connection command:
   ```
   claude mcp add windows-sandbox --transport sse --url http://<IP>:3232/sse
   ```

## Testing Workflow via MCP

Once connected, use MCPControl tools:

```
# Screenshot current state
mcp call windows-sandbox screenshot

# Type text
mcp call windows-sandbox type_text "test-project"

# Press keys
mcp call windows-sandbox press_key "Tab"
mcp call windows-sandbox press_key "Enter"
mcp call windows-sandbox press_key "j"  # navigation

# Run rahzom
mcp call windows-sandbox run_command "C:\rahzom\target\release\rahzom.exe"
```

## Prerequisites

1. Windows 10/11 Pro (Sandbox requires Pro+)
2. Windows Sandbox feature enabled:
   ```powershell
   Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM
   ```
3. Create `C:\sandbox-share` folder on host

## Verification

1. Run `/sandbox-wsb-init` — checks prerequisites
2. Launch .wsb file — Sandbox window opens
3. Wait for setup (~5-10 min first time)
4. Run connect.ps1 — get MCP URL
5. Connect Claude Code to MCP
6. Take screenshot — verify connection works
7. Launch rahzom, send keys, verify TUI responds
