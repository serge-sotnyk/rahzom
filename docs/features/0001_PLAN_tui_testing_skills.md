# TUI Testing Skills

Automated TUI testing infrastructure for rahzom using Docker (Linux) and Hyper-V (Windows) sandboxes. Enables Claude Code to test the application UI and file system operations in isolated environments.

## Skills to Create

Four Claude Code skills in `.claude/skills/`:

| Skill | Purpose |
|-------|---------|
| `sandbox-linux-init` | Initialize Docker container with Rust, tmux, built rahzom |
| `sandbox-linux-testing` | Execute TUI tests via tmux keystroke injection + screen capture |
| `sandbox-windows-init` | Check/prepare Hyper-V VM, provide setup instructions if needed |
| `sandbox-windows-testing` | Execute tests in Windows VM via PowerShell Direct |

## Phase 1: Linux Docker Skills

### sandbox-linux-init

**Files:**
- `.claude/skills/sandbox-linux-init/SKILL.md`
- `.claude/skills/sandbox-linux-init/Dockerfile`

**Dockerfile contents:**
- Base: `rust:latest`
- Install: tmux, python3-pip, ansi2html
- Workdir: `/app/rahzom`

**Skill workflow:**
1. Check if container `rahzom-test` exists
2. If not: build image from Dockerfile, create container with source volume mount
3. Start container if stopped
4. Create tmux session `main`
5. Build rahzom inside container (`cargo build --release`)
6. Report ready status

### sandbox-linux-testing

**Files:**
- `.claude/skills/sandbox-linux-testing/SKILL.md`
- `.claude/skills/sandbox-linux-testing/presets.md`

**Key commands:**
```bash
# Prepare test data
docker exec rahzom-test mkdir -p /test/left /test/right
docker exec rahzom-test sh -c 'echo "content" > /test/left/file.txt'

# Launch rahzom in tmux
docker exec rahzom-test tmux send-keys -t main './target/release/rahzom' Enter

# Send keys (j=down, k=up, Enter=select, Escape=back, q=quit)
docker exec rahzom-test tmux send-keys -t main 'j'

# Capture screen (text + ANSI codes)
docker exec rahzom-test tmux capture-pane -t main -p -e
```

**Test presets** (in presets.md):
- `basic_sync` - simple two-folder sync
- `conflict` - files modified on both sides
- `unicode` - Unicode filenames
- `long_names` - path length testing
- `large_tree` - performance testing (100+ dirs)

## Phase 2: Windows Hyper-V Skills

### sandbox-windows-init

**Files:**
- `.claude/skills/sandbox-windows-init/SKILL.md`
- `.claude/skills/sandbox-windows-init/setup-guide.md`

**Self-diagnosing workflow:**
1. Check Hyper-V cmdlets available → if not, print enable instructions
2. Check VM `RahzomTest` exists → if not, print VM creation guide
3. Check VM running → if not, start it
4. Check PowerShell Direct connectivity → if fails, print remoting setup
5. Check Rust installed → if not, run installer
6. Build rahzom → report status

**One-time setup (in setup-guide.md):**
1. Enable Hyper-V PowerShell management
2. Create VM via Hyper-V Manager (Windows 11 Eval ISO)
3. Enable PowerShell Remoting inside VM
4. Save credentials: `Export-Clixml "$env:USERPROFILE\.rahzom-vm-creds.xml"`
5. Create checkpoint `CleanState`

### sandbox-windows-testing

**Files:**
- `.claude/skills/sandbox-windows-testing/SKILL.md`
- `.claude/skills/sandbox-windows-testing/presets.md`

**Key commands:**
```powershell
$cred = Import-Clixml "$env:USERPROFILE\.rahzom-vm-creds.xml"

# Restore to clean state
Restore-VMCheckpoint -VMName RahzomTest -Name "CleanState" -Confirm:$false
Start-VM -Name RahzomTest

# Create test data
Invoke-Command -VMName RahzomTest -Credential $cred -ScriptBlock {
    New-Item -ItemType Directory -Force -Path C:\test\left, C:\test\right
}

# Run rahzom
Invoke-Command -VMName RahzomTest -Credential $cred -ScriptBlock {
    & C:\rahzom\target\release\rahzom.exe --help
}
```

**Note:** Windows TUI capture is complex (no tmux equivalent). Initial implementation focuses on build verification and file operations. Full TUI testing may require future `--headless` mode in rahzom.

## File Structure

```
.claude/skills/
├── sandbox-linux-init/
│   ├── SKILL.md
│   └── Dockerfile
├── sandbox-linux-testing/
│   ├── SKILL.md
│   └── presets.md
├── sandbox-windows-init/
│   ├── SKILL.md
│   └── setup-guide.md
└── sandbox-windows-testing/
    ├── SKILL.md
    └── presets.md
```

## Implementation Order

1. Create `sandbox-linux-init` (Dockerfile + SKILL.md)
2. Create `sandbox-linux-testing` (SKILL.md + presets.md)
3. Test Linux skills
4. Create `sandbox-windows-init` (SKILL.md + setup-guide.md)
5. Create `sandbox-windows-testing` (SKILL.md + presets.md)
6. Test Windows skills (requires one-time VM setup)
