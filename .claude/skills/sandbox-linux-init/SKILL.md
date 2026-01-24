---
name: sandbox-linux-init
description: Initialize Docker container for rahzom TUI testing on Linux. Use when setting up test environment, before running TUI tests, or when container needs rebuilding. Creates rahzom-test container with Rust, tmux, and built binary.
---

# Linux Sandbox Initialization

Initialize Docker container `rahzom-test` for TUI testing.

## Helper Scripts

This skill includes wrapper scripts that handle Git Bash path conversion issues:

- `docker-run.sh` - Create container with proper volume mount
- `docker-exec.sh` - Execute commands in container

**Add to allowed commands** for automation without prompts.

## Quick Start

### 1. Check Docker availability

```bash
docker --version
```

### 2. Check if container exists

```bash
docker ps -a --filter "name=rahzom-test" --format "{{.Names}}: {{.Status}}"
```

### 3. If container doesn't exist, build and create it

```bash
# Build image
docker build -t rahzom-test-image .claude/skills/sandbox-linux-init/

# Create container (use wrapper script)
.claude/skills/sandbox-linux-init/docker-run.sh
```

### 4. If container exists but stopped, start it

```bash
docker start rahzom-test
```

### 5. Create tmux session

```bash
.claude/skills/sandbox-linux-init/docker-exec.sh tmux kill-session -t main 2>/dev/null || true
.claude/skills/sandbox-linux-init/docker-exec.sh tmux new-session -d -s main -x 120 -y 40
```

### 6. Build rahzom

```bash
.claude/skills/sandbox-linux-init/docker-exec.sh cargo build --release
```

### 7. Verify setup

```bash
.claude/skills/sandbox-linux-init/docker-exec.sh ls -la /app/rahzom/target/release/rahzom
.claude/skills/sandbox-linux-init/docker-exec.sh tmux list-sessions
```

### 8. Tell user how to view the TUI

After setup is complete, inform the user:

> To watch the TUI in real-time, open a new terminal and run:
> ```bash
> docker exec -it -e LANG=C.UTF-8 -e LC_ALL=C.UTF-8 -e TERM=xterm-256color rahzom-test tmux attach -t main
> ```
> You'll see the same tmux session that I'm controlling. Both of us can see and interact with the TUI simultaneously.

## Cleanup Commands

```bash
docker stop rahzom-test
docker rm rahzom-test
docker rmi rahzom-test-image
```

## Troubleshooting

**Container won't start**: Check Docker daemon is running
**Build fails**: Ensure source is properly mounted, check cargo errors
**tmux errors**: Kill existing session first with `tmux kill-session -t main`

## Result

After successful init:
- Container `rahzom-test` is running
- tmux session `main` exists (120x40 terminal)
- rahzom binary at `/app/rahzom/target/release/rahzom`
- Test directories at `/test/left` and `/test/right`
