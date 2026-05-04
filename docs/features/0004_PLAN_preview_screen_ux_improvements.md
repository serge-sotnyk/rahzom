# Preview Screen UX Improvements

## Context

After running sync analysis, the Preview screen has several usability gaps that are visible in a real-world dataset (e.g. nested medical-document trees with deep paths and hash-named directories):

1. **Long paths get clipped** by ratatui at the right border, leaving rows like `files\sa_1029_010-009_02acc4...cdb853\crf\ecrfs\1029-010-009 ca` truncated mid-word, hiding the very part (filename) the user cares about.
2. **No sorting** — the order is whatever `differ` produced (directories first, then insertion order). It is hard to scan, find a specific file, or compare neighbouring entries.
3. **Default filter is `All`** — even when a project is in steady state with 0 actual changes, the user sees a wall of `Skip` entries instead of an empty list that immediately communicates "nothing to do".
4. **Left vs Right is invisible** — the row symbols (`→`, `←`) refer to "left side" and "right side" of the project, but those folders are only shown on the previous screen. On the Preview screen there is nothing reminding the user which physical folder is L and which is R, so `→` can be ambiguous.

This plan adds: middle-ellipsis truncation, a sort cycle, smarter default filter, a status bar with the selected full path, a details popup on Enter, an empty-list hint, and a folder-legend line that shows L/R paths visually aligned (left path left-justified, right path right-justified).

## Decisions (already taken with the user)

| Topic | Decision |
|---|---|
| Truncation | **Middle ellipsis** (`…`) on the whole rendered path, applied uniformly. If even the filename alone is wider than the column, middle-ellipsis the filename too. Computed in Unicode `chars()` (good enough for the ASCII/Latin paths this app handles). |
| Full path access | **Status bar** at the bottom of the Preview screen always shows the full selected path. **Enter** opens a details popup. |
| Default filter after analysis | **Always `Changes`**, even when empty (user can press F to switch). |
| Empty filter rendering | Empty list with a hint line inside the panel (`(no changes — press F to view all/skipped)`). |
| Sort | Cycle `Path → Type → Size`, default `Path`. Bound to **`O`** key. Header shows `Preview [Changes] [Sort: Path]`. |
| Sort: Path key | **Case-insensitive** comparison. |
| Sort: Type ordering | **Modified entries first, sorted by Type; then non-modified, sorted by Type.** Type order: `Conflict → Copy→ → Copy← → Delete → Skip`. Within a type, by path (case-insensitive). |
| State persistence | **In-memory only**, reset to defaults at every analysis. No config plumbing. |
| Cursor preservation | After sort/filter/edit, **track the previously selected path**; if it is no longer visible, snap to the nearest index that is. |
| Detail popup contents | Full path (wrapped) + action+direction (icon + text) + Left/Right metadata (size + mtime where present) + conflict/skip reason. Esc/Enter to close — no in-popup navigation/editing. |
| L/R legend | **One line above the action list** inside the `Actions` panel: left project path left-aligned, right project path right-aligned, with middle-ellipsis. If the terminal is too narrow for two readable halves, fall back to two stacked lines (one row each). No `L:` / `R:` labels — visual alignment carries the meaning. |
| Row layout | Inline (current shape): `[mark] symbol path [(size or reason)] [*]`. Just truncate path. |

## Implementation

### New module: `src/ui/text.rs`

Single helper used by row rendering, status bar, and L/R legend.

```rust
/// Middle-ellipsis truncation by Unicode chars.
/// If `s.chars().count() <= max`, returns `s` borrowed.
/// Otherwise returns "<head>…<tail>" with total chars == max.
/// Splits the budget so head gets ceil((max-1)/2) and tail gets the rest.
/// max < 2 → returns "…" (or empty if max == 0).
pub fn truncate_middle(s: &str, max: usize) -> Cow<'_, str>;
```

Tests in the same file: empty, ascii short, ascii long, max=0/1, mixed unicode.

### `src/sync/differ.rs`

Remove the unconditional dirs-first sort at lines 249–260. Sorting becomes a UI concern that depends on the user-chosen mode and on user modifications, not a property of the diff.

### `src/app/state.rs`

Add `PreviewSort`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewSort { Path, Type, Size }
impl Default for PreviewSort { fn default() -> Self { PreviewSort::Path } }
```

Extend `PreviewState`:

- `pub sort: PreviewSort` (default `Path`)
- A computed function `sorted_filtered_indices(&self) -> Vec<usize>` that:
  1. starts from the existing filter logic (`filtered_indices`),
  2. sorts those indices by current `sort`.

`sorted_filtered_indices` becomes the single source of truth for the rendered list. Replace direct `filtered_indices()` callers (rendering, navigation, scroll math, `selected_path()`).

Sort key derivation (helper functions on `UserAction` / via `PreviewState`):

- **Path**: compare `path.to_string_lossy()` case-insensitively (`.to_lowercase()` is fine — paths are ASCII-dominant here).
- **Type**: ordered tuple `(is_not_modified: bool, type_rank: u8, path_lowercase)`. `type_rank`: `Conflict=0, CopyToRight=1, CopyToLeft=2, CreateDirRight=1, CreateDirLeft=2, DeleteRight=3, DeleteLeft=3, Skip=4`. (Dirs piggy-back on copy ranks to keep their direction grouping.)
- **Size**: `(size_desc, path_lowercase)` — descending by size; non-files (Skip without size, dirs, conflicts without metadata) get size = 0 and sort to the end. Within equal size, by path.

Add `sort_label()` returning `"Path"|"Type"|"Size"`.

Cursor tracking on mutation/sort/filter:

- Add `pub fn capture_selected_path(&self) -> Option<PathBuf>`.
- Add `pub fn restore_selection_to_path(&mut self, path: Option<PathBuf>)` — sets `selected` to the index of `path` in the new sorted/filtered list, or to the smallest-index neighbour if missing, or to `0` if list empty.

Use this:
- around `cycle_filter`,
- around `cycle_sort` (new),
- around action-edit handlers (`change_action_to_left/right`, `skip_selected_action`, `reset_selected_action`).

Default initialization: when `PreviewState` is constructed at the end of analysis, set `filter = PreviewFilter::Changes` (today it’s `All`). Sort defaults to `Path`.

### `src/app/handlers.rs`

- Add `KeyCode::Char('o' | 'O')` on `Screen::Preview` → `cycle_sort()` (capture selected path, change sort, restore).
- Add `KeyCode::Enter` on `Screen::Preview` → open new `Dialog::ActionDetails(usize)` carrying the action index in `preview.actions` (resolve from current `selected` of sorted/filtered list).
- Update existing `cycle_filter` to use the capture/restore helpers.
- Update edit handlers to capture/restore around the mutation.
- Wire `Esc | Enter` while `Dialog::ActionDetails` is open → close dialog.

### `src/app/state.rs` — Dialog enum

Add variant: `Dialog::ActionDetails { action_index: usize }`.

### `src/ui/dialogs.rs`

New `render_action_details_dialog(frame, area, action, project, left_scan, right_scan)`:

- centered popup, ~70% width × min(15, content_lines + 4) height,
- content lines:
  1. Title (cyan) `Action details`.
  2. Full path (wrapped to popup width).
  3. Action and direction in human form, e.g. `→ Copy from C:\proj\src to D:\backup\dst` (uses real left/right paths from the project) and a `Modified by user` tag if `is_modified()`.
  4. Optional `Reason: <conflict reason or skip reason>`.
  5. Two metadata lines: `Left:  1.2 MB  2026-01-15 10:33` / `Right: 1.0 MB  2025-12-01 09:00`. If a side is missing, render `Left:  —`. mtime read from `left_scan` / `right_scan` (already on `PreviewState`).
- footer: `Esc/Enter: close`.

### `src/ui/screens.rs`

Replace `render_preview` body with this layout (top-to-bottom):

```
Constraint::Length(1)   // L/R legend (or 2 if narrow)
Constraint::Min(0)      // Actions list (with title and scrollbar)
Constraint::Length(1)   // Status bar: full path of selected
Constraint::Length(4)   // Summary (unchanged)
```

The legend area is inside the same outer block titled `Actions (X/Y) [Sort: <key>]`. Use ratatui's `block.title_top` for the right side, or bake `[Sort: …]` into the title string.

Implementation of legend:
- `cols = area.width as usize - 2` (account for borders).
- `min_per_side = 16`. If `cols >= 2 * min_per_side + 4` (separator/spacing): single line, left half = `truncate_middle(left, half)` left-aligned, right half = `truncate_middle(right, half)` right-aligned.
- Otherwise two stacked lines: left line left-aligned `truncate_middle(left, cols)`, right line right-aligned `truncate_middle(right, cols)`. Adjust `Constraint::Length` to 2 in that case (compute before splitting).

Row rendering changes (`render_action_item`):

- Compute `available = row_width - prefix_chars` where `prefix_chars` = 2 (mark) + 3 (symbol) + 1 (space).
- If row carries a trailing parenthesised tag (size or conflict reason), reserve its width too.
- Apply `truncate_middle(path_string, available)`.
- Keep all colours/styling exactly as today.

Status bar: single-line `Paragraph` rendering `truncate_middle(selected_full_path, area.width as usize - 2)` using a subtle dark-grey colour. When list empty → render an empty line (no path).

Empty-list hint:

- When `sorted_filtered_indices` is empty, render a single grey italic line inside the list area: `(no items match filter — press F to switch)`. Skip the scrollbar.

### `src/ui/widgets.rs`

If `truncate_middle` is small enough we can co-locate it here instead of `text.rs`. Decision: keep in a new `src/ui/text.rs` to keep `widgets.rs` focused on render helpers.

### Keyboard help line (`render_keyboard_help`-equivalent)

Add `O Sort` and `↵ Detail` to the hint row. (Find where today's `Esc Back` line lives — the screenshot shows it inside the bottom `Keyboard` panel; just append to the existing list.)

## Files to modify

- `src/ui/text.rs` — **new**, `truncate_middle` + tests.
- `src/ui/mod.rs` — `mod text;` + re-exports.
- `src/ui/screens.rs` — `render_preview`, `render_action_item`, status bar, L/R legend, empty-list hint, sort label in title.
- `src/ui/dialogs.rs` — `render_action_details_dialog`.
- `src/app/state.rs` — `PreviewSort`, `PreviewState.sort`, `sorted_filtered_indices`, `capture_selected_path` / `restore_selection_to_path`, `Dialog::ActionDetails`, default filter = `Changes`.
- `src/app/handlers.rs` — sort cycle key, Enter → details dialog, Esc/Enter on dialog, capture/restore around mutations.
- `src/sync/differ.rs` — remove the dirs-first sort at lines 249–260.
- `src/app/mod.rs` — title string update (`[Sort: …]`), routing for the new dialog.

## Verification

1. **Unit tests** in `src/ui/text.rs`: `truncate_middle` covers empty, fits-as-is, ASCII long, very small `max` (0/1/2), unicode mix.
2. **Unit tests** in `src/app/state.rs`: build a `PreviewState` with a synthetic mix (Conflict + Copy→ + Copy← + Delete + Skip + Modified-overrides) and assert `sorted_filtered_indices()` order for `Path`, `Type`, `Size`. Add a test that mutating an action keeps cursor on the same path; another that toggling filter preserves the path when present and falls back when not.
3. **TUI sandbox tests** (Linux + Windows skills already exist):
   - Run analysis on a tree that reproduces the screenshot (deep `sa_1029…` paths). Confirm:
     - opens to `Changes` filter,
     - paths appear with middle ellipsis,
     - L/R legend renders correctly on a wide and a narrow terminal (resize),
     - status bar updates as you press ↑/↓,
     - `Enter` opens the details popup, `Esc` closes,
     - `O` cycles sort, the `[Sort: …]` label updates, and the cursor stays on the same file,
     - `F` cycles filter, cursor stays on the same file when the file is still visible,
     - empty Changes list shows the hint instead of just blank rows,
     - editing with `←`/`→`/`S` does not lose the selected file when it changes type.
4. `cargo fmt && cargo clippy --all-targets -- -D warnings` clean.
5. `cargo test` passes.

## Out of scope (explicitly)

- Persisting sort/filter across runs — kept ephemeral on purpose.
- Bulk operations on multi-selected items (the `HashSet<usize>` machinery exists but is not wired; do not extend it here).
- Completing the half-implemented `R` reset — leave as-is unless trivially affected by these changes.
- Horizontal scroll, two-line wrap, or column header sorting (rejected during interview).

## Post-implementation revisions

After visual testing we decided that the dedicated status-bar row showing the
full path of the selected action is redundant — the in-list rows already use
middle ellipsis to keep the most informative parts of the path visible, and
the dialog reachable via Enter shows the full path anyway. The status-bar
element only consumed vertical space without paying its rent, so it was
removed from the Preview screen layout. The plan and implementation kept
everything else (legend, sort cycle, default `Changes` filter, Enter detail
popup, empty-list hint).
