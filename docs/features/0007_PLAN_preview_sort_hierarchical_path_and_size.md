# Preview sort: hierarchical Path and meaningful Size

## Context

On the Preview screen, the `O` key cycles sort modes Path → Type → Size (`src/app/handlers.rs:604-611`, `src/app/state.rs:613-633`). In the user's screenshot, both folders are identical → all 15 actions are `Skip`. The current implementation produces an identical order in all three modes:

- `path_key` is a plain `to_string_lossy().to_lowercase()` with no awareness of file vs. directory → within a single folder, subfolders may appear before or after files depending on lexicographic order (e.g. `z.txt` after the `a/` folder).
- `type_rank` — every Skip = 4, tie-break by path → order matches Path mode.
- `action_size` — for every non-Copy action returns 0 (`state.rs:599-607`), tie-break by path → again identical to Path.

In addition, `SyncAction::Skip` (`src/sync/differ.rs:54`) and `SyncAction::DeleteLeft/Right` (`differ.rs:39-41`) carry neither `size` nor `is_directory`. Without those fields, sorting Skip/Delete by size is physically impossible, and the "files-before-folders" rule for Path can't be expressed either.

The goal is two changes:
1. **Path**: hierarchical sort with the rule "within each folder, files come before subfolders, and a folder entry is immediately followed by its contents recursively".
2. **Size**: real size-based sort; for folders — total subtree size; conflict — `max(left, right)`; tie-break — hierarchical Path.

## Design

### Model extension

**`SyncAction::Skip`** (`src/sync/differ.rs`): add `size: u64` and `is_directory: bool`. The differ already owns this information — `FileEntry { size, is_dir, .. }` (`differ.rs:110-115`).

**`SyncAction::DeleteLeft / DeleteRight`** (`differ.rs:39-41`): add `size: u64` and `is_directory: bool`. The differ builds them from `FileEntry`, the fields are available.

**`SyncAction::Conflict`** can fire for directories too (e.g. `differ.rs:279-280` `l.is_dir != r.is_dir`, `351` exists-vs-deleted on a dir). To answer "is this conflict on a directory?" without losing precision, add `is_directory: bool` to `FileInfo` — that lets the conflict carry the same flag from either side, and surfaces the type-mismatch case naturally (`left.is_directory != right.is_directory`). `is_directory(action)` for Conflict = `left.is_directory || right.is_directory` (an `Option<FileInfo>` that is `None` contributes `false`).

**`UserAction::DeleteLeft / DeleteRight / Skip`** (`src/app/state.rs:309-314`): extend in lockstep with `size: u64` and `is_directory: bool`. Critically, when the user converts an `Original` action into a manual `Skip`/`DeleteLeft`/`DeleteRight` (`src/app/handlers.rs:648-686`), preserve `size`/`is_directory` from the original action — otherwise pressing Skip on a large file or a folder would silently zero out its size and break Size sort.

`CreateDirLeft/Right` — by definition `is_directory = true`, `size = 0` (an empty folder is created); subtree size is computed from siblings.
`CopyToLeft/Right` — files (if copying empty folders is added later, it will be implemented as `CreateDir*`).

Also update `to_sync_action`, `is_skip_action`, the tests in `differ.rs` (775-1062), `executor.rs::sort_actions` (`src/sync/executor.rs:218`) — everywhere the current pattern matches.

### `is_directory(action) -> bool`

Helper in `app/state.rs`:
```rust
fn is_directory(a: &UserAction) -> bool {
    use crate::sync::differ::SyncAction;
    match a {
        UserAction::Original(SyncAction::CreateDirLeft { .. })
        | UserAction::Original(SyncAction::CreateDirRight { .. }) => true,
        UserAction::Original(SyncAction::Skip { is_directory, .. })
        | UserAction::Original(SyncAction::DeleteLeft { is_directory, .. })
        | UserAction::Original(SyncAction::DeleteRight { is_directory, .. })
        | UserAction::Skip { is_directory, .. }
        | UserAction::DeleteLeft { is_directory, .. }
        | UserAction::DeleteRight { is_directory, .. } => *is_directory,
        UserAction::Original(SyncAction::Conflict { left, right, .. }) => {
            left.as_ref().map(|f| f.is_directory).unwrap_or(false)
                || right.as_ref().map(|f| f.is_directory).unwrap_or(false)
        }
        UserAction::Original(SyncAction::CopyToRight { .. })
        | UserAction::Original(SyncAction::CopyToLeft { .. })
        | UserAction::CopyToRight { .. }
        | UserAction::CopyToLeft { .. } => false,
    }
}
```

### Hierarchical Path

Replace `path_key` with a structural key. For each path build a `Vec<(u8, String, u8)>`, one triple per path component:

- First slot — **kind at this position**: `0` if the component represents a file (only valid when this is the leaf), `1` if it represents a directory (leaf or intermediate).
- Second slot — **lower-cased component name**.
- Third slot — **leaf marker**: `0` if this is the last component of the path, `1` if more components follow.

Triples are compared lexicographically; the resulting `Vec` is then compared lexicographically.

Why a triple, not a pair: the earlier draft used `(intermediate?, name)` and was wrong. With `[(1,"a")]` for the dir-leaf `a/` and `[(2,"a"),(0,"x.txt")]` for `a/x.txt`, comparing `[(1,"b")]` (dir-leaf `b/`) against `[(2,"a"),(0,"x.txt")]` gives `1<2` → `b/` ranks before `a/x.txt`, which breaks "folder entry immediately followed by its contents". Putting `name` ahead of the leaf flag in the same triple fixes this: the dir name is compared before we ever notice whether we're at the leaf or going deeper.

Worked example for the order
```
z.txt
a/
a/x.txt
a/sub/
a/sub/y.txt
b/
b/q.txt
```

| Path           | Key                                                              |
|----------------|------------------------------------------------------------------|
| `z.txt`        | `[(0,"z.txt",0)]`                                                |
| `a/`           | `[(1,"a",0)]`                                                    |
| `a/x.txt`      | `[(1,"a",1), (0,"x.txt",0)]`                                     |
| `a/sub/`       | `[(1,"a",1), (1,"sub",0)]`                                       |
| `a/sub/y.txt`  | `[(1,"a",1), (1,"sub",1), (0,"y.txt",0)]`                        |
| `b/`           | `[(1,"b",0)]`                                                    |
| `b/q.txt`      | `[(1,"b",1), (0,"q.txt",0)]`                                     |

Lexicographic sort over these keys yields exactly the listed order.

Spot-checks:
- `z.txt` vs `a/`: first kind `0` vs `1` → root file before any folder. ✓
- `a/` vs `a/x.txt`: first triple equal on kind+name, leaf marker `0` vs `1` → folder entry before its contents. ✓
- `b/` (`(1,"b",0)`) vs `a/x.txt` (`(1,"a",1)…`): equal kind, name `"b" > "a"` → `a/x.txt` before `b/`. ✓ (folder `a/` and its whole subtree close out before `b/` opens)
- `a/x.txt` vs `a/sub/`: equal first triple, then second triple `(0,"x.txt",0)` vs `(1,"sub",0)` → file before subfolder at the same level. ✓
- Empty subfolder `c/` (no children) sits as `[(1,"c",0)]` and falls into its alphabetical slot among siblings.

Component name comparison is case-insensitive (`to_lowercase()`).

### Size

New `effective_size(action) -> u64`:
- `CopyToRight/Left` (Original or User): `size`.
- `Skip` (file): `size`. `Skip` (dir): aggregated subtree size.
- `DeleteRight/Left` (file): `size`. (dir): aggregated subtree size.
- `Conflict`:
  - if `is_directory(action)` is `false` (both present sides are files): `max(left.size, right.size)`, treating `None` as 0.
  - if `is_directory(action)` is `true` (at least one side is a directory — covers dir-vs-dir and file-vs-dir type-mismatch conflicts): `max(file_side_size, aggregated_subtree_size)`. This way a "small file vs huge directory" conflict ranks by the heavier side instead of getting buried under the file size alone.
- `CreateDirLeft/Right`: aggregated subtree size.

**Aggregated subtree size**: for each dir path, the sum of `effective_size` of all file-actions whose path starts with `dir_path` followed by the platform separator. Cache in `PreviewState` as `dir_sizes: HashMap<PathBuf, u64>`. Compute in two passes:
1. First pass: for every file-action collect `(path, size)`.
2. Second pass: walk each `(path, size)` up its ancestors and add `size` to every ancestor that is itself a dir-action path. (Dir-action paths are gathered up-front into a `HashSet`.)

This is O(N · depth), no nested scans of the action vector.

**Architecture: do NOT physically sort `preview.actions`.** `actions` is stable storage. View order is produced by `sorted_filtered_indices()` (`state.rs:419`), and `selected_items: HashSet<usize>` (`state.rs:380`) plus all handler callsites (`handlers.rs:170, 179, 523, 580, 615, 626-631, 640, 661, 682, 695`) hold *real* indices into `actions`. Reordering `actions` in place would silently break selection and any persistent index. The cache and the sort therefore split into two distinct concerns:

- `dir_sizes` is the only thing that needs to be recomputed when actions change (mutations can change a file's `effective_size`, e.g. resolving a Conflict to one specific side).
- The sort itself stays inside `sorted_filtered_indices()`, which now consults `self.dir_sizes` and `self.sort` to compute view order on demand.

`recompute_view()` rebuilds `dir_sizes` only — it does not touch `actions`. `cycle_sort` simply updates `self.sort` and does *not* call `recompute_view()` (the cache is sort-independent).

**Where to call `recompute_view()`**: encapsulate every action mutation in `PreviewState`, keep `recompute_view()` a private method, and make the constructor and the mutation methods the only callers.

- `PreviewState::new(...)` builds `actions` and then calls `self.recompute_view()` before returning. Callsites (`mod.rs:250`, `mod.rs:1185`, future tests) get a correct state with no extra step.
- Add a public mutation API to `PreviewState`:
  ```rust
  pub fn replace_action(&mut self, real_idx: usize, new_action: UserAction);
  ```
  Internally: `self.actions[real_idx] = new_action; self.recompute_view();`. Handlers (`handlers.rs:648-686`) switch from `preview.actions[real_idx] = …` to `preview.replace_action(real_idx, …)`. Direct field access from outside is no longer needed for mutation, so the recompute step can never be forgotten.
- Test helper `mk_state` in `state.rs:640-645` currently bypasses recompute (`PreviewState { actions, ..Default::default() }`). Either route it through `PreviewState::new` (build a synthetic `DiffResult`) or call `state.recompute_view()` after construction — pick whichever is shorter for the existing test surface and keep the `recompute_view()` invocation explicit.

Sort: `effective_size(b).cmp(&effective_size(a))` (descending), tie-break — hierarchical Path key. Implemented inside `sorted_filtered_indices()` by sorting the index vector with a comparator that reads `self.actions[i]` and `self.dir_sizes`.

### Where to edit

Critical files:
- `src/sync/differ.rs` — extend `SyncAction::Skip / DeleteLeft / DeleteRight` (fields + constructors in `diff(...)`); add `is_directory` to `FileInfo`; update tests ~775-1062.
- `src/app/state.rs` — `UserAction` (309-314), `to_sync_action` (336-355), `is_skip_action` (569-573), replace `path_key` / re-shape `sort_key_cmp` (599-633), `effective_size`, `dir_sizes` cache, private `recompute_view()` and public `replace_action()` on `PreviewState`, the `is_directory` helper, sort wired through `sorted_filtered_indices()`. Update `mk_state` test helper (640-645) so it doesn't bypass recompute.
- `src/app/handlers.rs` — preserve `size`/`is_directory` when converting Original → manual Skip/Delete (lines 648-686); replace direct `preview.actions[real_idx] = …` writes with `preview.replace_action(real_idx, …)` so recompute happens automatically. No direct call to `recompute_view()` from handlers.
- `src/app/mod.rs` — nothing to do at `PreviewState::new` callsites (`mod.rs:250`, `mod.rs:1185`): the constructor handles recompute itself. Leave the executor's `sort_actions` at `mod.rs:298` alone.
- `src/sync/executor.rs::sort_actions` (218) — update Skip/Delete patterns to the new shape.
- `src/ui/screens.rs::decompose_action` (362-416) — show effective size as the trailing tag for Skip and Delete actions when `> 0`. For directories the tag is the aggregated subtree size. This makes Size-sort changes visible: today Skip rows render as just `· path`, with no size, so reordering looks like a no-op even when it isn't. Implementation: pass an `effective_size: u64` (precomputed via `PreviewState::effective_size(action)`) into `decompose_action` alongside the action, format with `format_bytes` when `> 0`. Copy actions keep their existing size tag; Conflict keeps its reason tag.
- All `match` arms over `SyncAction::Skip { path, reason }` / `Delete*` → ignore the new fields with `..` where the meaning is preserved.

### Verification

1. `cargo fmt && cargo clippy --all-targets -- -D warnings`.
2. `cargo test` — extend the unit tests in `state.rs` (`mod tests` from line 635):
   - **Path tree-order test**: input set
     ```
     z.txt, a/, a/x.txt, a/sub/, a/sub/y.txt, b/, b/q.txt
     ```
     after Path sort comes out in exactly that order, regardless of input permutation.
   - **Files-before-folders at same level**: `z.txt` (root file) sorts before `a/` (root dir) even though `'z' > 'a'`.
   - **Folder entry hugs its contents**: `a/` is immediately followed by `a/x.txt`, `a/sub/`, `a/sub/y.txt` before any `b/...` appears.
   - **Size sort on identical folders**: Skip-files sort by their real `size` descending; a Skip-dir sorts by the aggregate of its subtree, not 0.
   - **Manual Skip preserves size**: converting a `CopyToRight { size: 1_000_000 }` into `UserAction::Skip { … }` via `replace_action` keeps `size = 1_000_000` and `is_directory = false`; Size sort still places it near the top.
   - **Selection survives sort cycle**: select two non-adjacent rows via `selected_items`, then call `cycle_sort` through every mode — `selected_items` still points at the same `UserAction` entries (paths unchanged). This guards against any future regression to in-place sort of `actions`.
   - **`replace_action` rebuilds dir aggregates**: a Conflict whose `max(left,right)` = 5 MB is contained in dir `D` (subtree size 10 MB). Resolve it to `CopyToLeft { size: 1 MB }` via `replace_action` — assert `dir_sizes[D] = 6 MB` afterwards.
   - **Conflict on dir**: a `Conflict` whose `left.is_directory = true` reports `is_directory = true` and uses subtree size, not 0.
3. Sandbox Linux/Windows: run the TUI on two identical folders with nesting, verify:
   - Path: root files → each subfolder with its files → its subfolders recursively.
   - Size: large files at the top; folders sort by aggregate; tie-break by Path.
   - Type: modified at the top, then groups by rank.

## Decisions (confirmed by the user)

- **Folder size** = aggregated subtree size (as in Total Commander Alt+Shift+Enter / Beyond Compare).
- **Conflict size** = `max(left.size, right.size)` when both sides are files; `max(file_side_size, aggregated_subtree_size)` when at least one side is a directory.
- **Stable storage** = `preview.actions` is never reordered. Sort lives inside `sorted_filtered_indices()`; `dir_sizes` is the only cached state.
- **Recompute ownership** = `PreviewState::new` and `replace_action` are the only mutators, both call private `recompute_view()`. Callsites and handlers do not call recompute themselves.
- **UI**: Skip/Delete rows show effective size as a trailing tag when `> 0` (aggregate for dirs), so Size-sort changes are actually visible.
