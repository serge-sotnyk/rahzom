---
name: sandbox-linux-testing
description: Execute TUI tests for rahzom in Linux Docker container. Use for testing UI interactions, keyboard navigation, screen rendering, sync workflows. Requires sandbox-linux-init to be run first.
---

# Linux Sandbox Testing

Execute TUI tests in the `rahzom-test` Docker container.

**Prerequisite**: Run `/sandbox-linux-init` first to set up the container.

## Shared Console (like Windows)

Before testing, remind the user to attach to the tmux session so they can watch in real-time:

> To watch the TUI, run in your terminal:
> ```bash
> docker exec -it -e LANG=C.UTF-8 -e LC_ALL=C.UTF-8 -e TERM=xterm-256color rahzom-test tmux attach -t main
> ```
> You'll see the same session I'm controlling. Commands I send via `tmux send-keys` will appear in your terminal.

**Note:** The `-e` flags are required for proper UTF-8 box-drawing characters display in Windows Terminal.

Once the user is attached, they see exactly what the agent sees - just like the Windows console-bridge approach.

## Helper Script

Use the wrapper script from sandbox-linux-init for all docker exec commands:

```bash
# Shorthand alias for this session
alias dexec='.agents/skills/sandbox-linux-init/docker-exec.sh'

# Or use full path
.agents/skills/sandbox-linux-init/docker-exec.sh <command>
```

## Test Workflow

### 1. Prepare test data

Choose a preset from [presets.md](presets.md) or create custom data:

```bash
# Clear previous test data
.agents/skills/sandbox-linux-init/docker-exec.sh rm -rf /test/left/* /test/right/*

# Example: basic_sync preset
.agents/skills/sandbox-linux-init/docker-exec.sh sh -c 'echo "shared content" > /test/left/shared.txt'
.agents/skills/sandbox-linux-init/docker-exec.sh sh -c 'echo "shared content" > /test/right/shared.txt'
.agents/skills/sandbox-linux-init/docker-exec.sh sh -c 'echo "left only" > /test/left/left_only.txt'
.agents/skills/sandbox-linux-init/docker-exec.sh sh -c 'echo "right only" > /test/right/right_only.txt'
```

### 2. Clear rahzom config (fresh start)

```bash
.agents/skills/sandbox-linux-init/docker-exec.sh rm -rf /root/.rahzom
```

### 3. Launch rahzom in tmux

```bash
# Ensure clean tmux state
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main C-c
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'clear' Enter

# Launch rahzom
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main '/app/rahzom/target/release/rahzom' Enter

# Wait for startup
sleep 1
```

### 4. Capture screen

```bash
# Text with ANSI codes (the agent can interpret colors)
.agents/skills/sandbox-linux-init/docker-exec.sh tmux capture-pane -t main -p -e
```

### 5. Send keystrokes

```bash
# Navigation
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'j'        # Down
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'k'        # Up
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'Enter'    # Select/Confirm
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'Escape'   # Back/Cancel

# Actions
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'n'        # New project
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'a'        # Analyze
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 's'        # Sync
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'q'        # Quit

# Text input (for dialogs)
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'test-project'
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'Tab'      # Next field

# Special keys
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'BSpace'   # Backspace
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'DC'       # Delete
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'Left'     # Arrow left
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'Right'    # Arrow right
```

### 6. Quit application

```bash
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'q'
# Or force quit
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main C-c
```

## Common Test Scenarios

### Create new project and analyze

```bash
# Start fresh
.agents/skills/sandbox-linux-init/docker-exec.sh rm -rf /root/.rahzom

# Launch
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main '/app/rahzom/target/release/rahzom' Enter
sleep 1

# Press 'n' for new project
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'n'
sleep 0.5

# Enter project name
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'test-project'
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'Tab'

# Enter left path
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main '/test/left'
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'Tab'

# Enter right path
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main '/test/right'
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'Enter'

# Capture result
sleep 0.5
.agents/skills/sandbox-linux-init/docker-exec.sh tmux capture-pane -t main -p -e
```

### Navigate preview and change actions

```bash
# From project view, press 'a' to analyze
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'a'
sleep 1

# Capture preview
.agents/skills/sandbox-linux-init/docker-exec.sh tmux capture-pane -t main -p -e

# Navigate with j/k, change action with arrows
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'j'      # Move down
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'Right'  # Change to copy right
.agents/skills/sandbox-linux-init/docker-exec.sh tmux send-keys -t main 'Left'   # Change to copy left
```

## Verifying Results

After test actions, verify by:

1. **Screen capture**: Check UI shows expected state
2. **File system**: Check files in /test/left and /test/right
3. **Metadata**: Check /test/left/.rahzom and /test/right/.rahzom

```bash
# Check files
.agents/skills/sandbox-linux-init/docker-exec.sh ls -la /test/left/
.agents/skills/sandbox-linux-init/docker-exec.sh ls -la /test/right/

# Check metadata
.agents/skills/sandbox-linux-init/docker-exec.sh ls -la /test/left/.rahzom/ 2>/dev/null || echo "No metadata yet"
```

## Tips

- Always capture screen after each action to verify state
- Use `sleep 0.5` between rapid keystrokes for UI to update
- Clear `/root/.rahzom` for fresh project list
- Clear `/test/left/.rahzom` and `/test/right/.rahzom` for fresh sync state
