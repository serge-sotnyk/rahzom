# Quiet console-bridge: redirect runtime logs to a file

## Context

The Windows sandbox testing skill drives a `rahzom` TUI through `console-bridge.exe`, a small Rust helper that injects keystrokes (via `WriteConsoleInputW`) and dumps the screen buffer (via `ReadConsoleOutputW`) into `C:\rahzom-test\.bridge-screen` on demand.

Today the bridge logs status messages with `println!`/`eprintln!` *after* it has spawned the child rahzom process — including a chatty `[console-bridge] Screen captured to C:\rahzom-test\.bridge-screen` line on every `capture` command. Both stdout and stderr on Windows write into the **same console screen buffer** that ratatui draws into, so those log lines:

1. Land visibly inside the rahzom TUI window. On lightly-populated screens they end up below the bottom border (visible on the user's screenshot of the Projects screen). On dense screens (e.g. the Preview screen with deep paths and the L/R legend) the log text overwrites parts of the TUI and creates the "месиво" (mess) the user described.
2. Bleed into subsequent `.bridge-screen` captures because ratatui only redraws diffs — log lines that fall outside the changed regions persist in the buffer until they are scrolled or until the next full repaint.

The fix is to stop writing anything to the console once the child has been spawned. Runtime messages will go to a log file instead, where the agent can still read them when debugging.

## Skill layout (where the source actually lives)

```
.agents/skills/sandbox-windows-init/
├── SKILL.md
├── build.ps1
├── setup-user.ps1
└── console-bridge/
    ├── Cargo.toml
    ├── Cargo.lock
    └── src/main.rs                 ← only file with code changes

.agents/skills/sandbox-windows-testing/
├── SKILL.md                         ← documentation update (mention .bridge-log)
└── presets.md

.claude/skills/sandbox-windows-{init,testing}/SKILL.md
    ← stubs that just point to .agents/...; no edits needed
```

Verified via `diff -rq` — the `.claude` copies are stub `SKILL.md` files; the bridge source and the real instructions only exist under `.agents/`.

## Decision

| Question | Decision |
|---|---|
| Stderr or log file? | **Log file.** On Windows cmd/Windows Terminal, stderr shares the screen buffer with stdout; `eprintln!` would not actually help. |
| Log path | `C:\rahzom-test\.bridge-log` (sibling of the existing `.bridge-commands` and `.bridge-screen`). Hard-coded constant, same as the others. |
| Truncate vs append on startup | **Truncate** at bridge startup. Each session gets a fresh log; no disk-utility growth across sessions. |
| Append within a session | Yes — open with `append(true)` for each write, simplest correct behavior in this single-threaded loop. |
| Pre-spawn messages | Move them to the log too. ratatui clears the screen on init via the alternate-screen sequence, but keeping behavior uniform avoids future regressions if the launch order ever changes. |
| Usage / help (when args missing) | Keep `eprintln!` — child has not spawned yet, no TUI to corrupt, useful when invoked manually. |
| Timestamps in log lines | None. Keeps the helper trivial (no extra crate). The log is read for "what did the bridge do" not "when". |

## Implementation

### `.agents/skills/sandbox-windows-init/console-bridge/src/main.rs`

1. Add a `LOG_FILE` constant next to the existing `CMD_FILE` / `SCREEN_FILE`:
   ```rust
   const LOG_FILE: &str = r"C:\rahzom-test\.bridge-log";
   ```

2. Add a tiny helper that opens the file, writes a line, and swallows errors (logging must never panic the bridge):
   ```rust
   fn log(msg: &str) {
       use std::io::Write;
       if let Ok(mut f) = std::fs::OpenOptions::new()
           .create(true).append(true).open(LOG_FILE)
       {
           let _ = writeln!(f, "{}", msg);
       }
   }
   ```

3. At startup (right where the command file is cleared), also reset the log file: `let _ = fs::write(LOG_FILE, "");`.

4. Replace every runtime `println!`/`eprintln!` that fires *after* the child is spawned with `log(...)`:
   - L60–61 startup banner ("Starting", "Listening")
   - L79  `Child exited with`
   - L84  `Error checking child`
   - L102 `Exit command received`
   - L108 `Error processing`
   - L142 `Screen captured to ...`

   Note: although L60–61 fire before `child.spawn()`, route them to the log too for uniformity (ratatui clears the screen anyway, but if the spawn order ever changes we don't want to reintroduce the bug).

5. Leave the args-missing usage block (L40–51) on `eprintln!`. The child never starts on that path.

6. Leave the `#[cfg(not(windows))]` `eprintln!` (L120) — non-Windows builds don't drive a TUI and the message is the only signal the user gets that the binary won't work.

That's the entire code change — about 10 line edits, no new dependencies.

### `.agents/skills/sandbox-windows-testing/SKILL.md`

Add a short subsection (under "Screen Capture" or just after the bridge-commands table) explaining that `C:\rahzom-test\.bridge-log` now contains bridge-internal messages (start/stop, errors, capture confirmations) and is safe to `Get-Content` for debugging without disturbing the TUI.

No changes to `presets.md`, the `.claude` stub files, `build.ps1`, or `setup-user.ps1`.

## Files to modify

- `.agents/skills/sandbox-windows-init/console-bridge/src/main.rs` — log helper + replace runtime prints.
- `.agents/skills/sandbox-windows-testing/SKILL.md` — document `.bridge-log`.

## Verification

1. Ask the user to send `exit` to the running bridge (or close the cmd window) so `console-bridge.exe` is no longer locked.
2. Rebuild and redeploy: `.\\.agents\\skills\\sandbox-windows-init\\build.ps1 -BridgeOnly`. This drops a fresh `console-bridge.exe` into `C:\rahzom-test\bin\`.
3. Ask the user to relaunch:
   `runas /user:rahzom-tester "cmd /k cd C:\rahzom-test && bin\console-bridge.exe bin\rahzom.exe"`
4. Trigger several captures and a few interactions, including the Preview screen with deep paths (the test fixtures from the previous task are still under `C:\rahzom-test\left` / `right`).
5. Check the consoles:
   - The cmd window must not show any `[console-bridge] ...` lines after rahzom takes over the screen.
   - `Get-Content C:\rahzom-test\.bridge-log` must contain entries like `Starting: ...`, `Listening for commands on: ...`, and one `Screen captured to ...` per capture.
6. Re-capture the Preview screen with the long `aaa...aaa.pdf` row from the previous task and confirm there are no `[console-bridge]` artifacts in `.bridge-screen` and no orphan log lines visible in the cmd window.

## Out of scope

- Eliminating `child.try_wait()` polling / converting the loop to async — current 100 ms polling is fine.
- Adding timestamps or log levels — over-engineering for a developer-only debug log.
- Changing the bridge command protocol or capture format.
- Touching the Linux equivalent skill (it uses a different mechanism — `tmux` capture-pane — and has no analogous problem).
