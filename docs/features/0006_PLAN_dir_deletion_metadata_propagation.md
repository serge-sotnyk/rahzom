# Fix: deletion of directories is not propagated through metadata

## Context

The user reproduced the following scenario:
1. An empty subfolder `folder/` is created in one of the two synchronized folders.
2. Sync — both sides receive `folder/` (the executor performs `CreateDirRight`).
3. The user deletes `folder/` on one side.
4. After analysis, rahzom proposes to **copy the folder back**, instead of deleting it on the other side as well.

The user's expectation was that metadata about the folder is recorded in `.rahzom/state.json`, so the deletion should be detected and propagated.

The root cause is two related gaps in the metadata architecture:

1. **Metadata is not written for directories.** In `src/app/mod.rs:558-630` the `save_sync_metadata` function only handles `CopyToRight/Left` (via `upsert_file`) and `DeleteRight/Left` (via `mark_deleted`). The `CreateDirRight/CreateDirLeft` branches fall into `_ => {}` (line 628), so after the first sync `folder/` does not appear in either `state.json`.

2. **Differ does not consult metadata for directories.** In `src/sync/differ.rs:318-321` and `:365-366` the "exists only on left/right" branches for directories **immediately** return `CreateDirRight`/`CreateDirLeft`, without looking at `*_prev` or `*_deleted`. The whole "was synchronized → now gone on one side → propagate deletion" logic works only for files (lines 335-353 and 381-400).

So even if metadata contained a record for the directory, the current differ would still propose to recreate it. Symmetrically, if `folder/` contained a file `a.txt` and the user deleted the whole tree — `a.txt` is marked as a deleted file and removed from the other side, but `folder/` itself gets recreated (see action ordering in `executor.rs:282-298`: `CreateDir` runs before `Delete`, so the net effect is that both sides end up with an empty `folder/`).

`FileState` and `DeletedFile` (`src/sync/metadata.rs:75-107`) currently do not distinguish between a file and a directory at all (no `is_dir` field).

## Goal

After the next sync:
- An empty subfolder that was created and synchronized is recorded in metadata on both sides.
- When such a subfolder is later deleted on one side, the differ proposes `DeleteRight`/`DeleteLeft` for the other side.
- Deletion of a non-empty subtree (`folder/` + its contents) propagates as a deletion correctly, without "recreating" the parent directory.
- Old `state.json` files without `is_dir` load without errors (they are treated as files — preserving current behavior for existing records).

## Changes

### 1. `src/sync/metadata.rs` — mark whether a record refers to a directory

- Add `pub is_dir: bool` to both `FileState` and `DeletedFile`, with `#[serde(default)]`, so that existing `state.json` files load without errors (old records will get `is_dir = false` — i.e. treated as files, as today).
- Update test constructors `sample_file_state` / `sample_deleted_file` (and `differ.rs::tests::make_file_state`) to set `is_dir: false`.
- No new public methods are introduced — `find_file` / `find_deleted` already return `&FileState` / `&DeletedFile`, callers can read the new field directly.

### 2. `src/app/mod.rs` — record directories and label them correctly on deletion

Extend `save_sync_metadata` (lines 558-630):

- Add branches for `SyncAction::CreateDirRight { path }` and `SyncAction::CreateDirLeft { path }`. For both:
  - construct `FileState { is_dir: true, size: 0, mtime: now, attributes: …, hash: None, last_synced: now, path }` and call `upsert_file` on **both** metadata sides (mirroring how copies are handled today, see `:580-581`).
- For `DeleteRight { path }` / `DeleteLeft { path }`, determine whether the action targeted a directory. The simplest source of truth is the **previous** metadata on the same side: if `prev.find_file(path)` returns a record with `is_dir = true`, then `DeletedFile.is_dir = true`. If there is no record — fall back to `is_dir = false` (current behavior). The `prev` metadata is already loaded on lines 550-553.

No new disk reads are needed for directories — `mtime/size` of directories are not used by the differ.

### 3. `src/sync/differ.rs` — consult prev/deleted for directories

In `determine_action` update two branches.

**Branch `(Some(l), None)` (lines 318-361):** remove the early `return SyncAction::CreateDirRight` for directories and rewrite so that the common logic also covers directories. Pseudo-code:

```rust
(Some(l), None) => {
    // Try to figure out whether the path was previously synchronized.
    // For a directory "modified on left" does not really make sense —
    // we treat a directory as unmodified as long as it just exists.
    let was_synced_on_right = right_prev.is_some();
    let was_deleted_on_right = right_deleted; // already considered for files

    if l.is_dir {
        if was_deleted_on_right || was_synced_on_right {
            // The directory used to be common, now gone on the right —
            // delete it on the left as well.
            // (If right_prev was a file — that is a type change; for now we
            //  still propose DeleteLeft, which is safe: there's nothing of
            //  value on the left except the empty/recreated dir. Type-change
            //  conflicts can be tightened up later.)
            return SyncAction::DeleteLeft { path: path_buf };
        }
        return SyncAction::CreateDirRight { path: path_buf };
    }

    // below — existing file logic, unchanged
    if right_deleted { … }
    else if right_prev.is_some() { … }
    else { CopyToRight }
}
```

**Branch `(None, Some(r))` (lines 363-408):** symmetric — for `r.is_dir` consult `left_deleted` / `left_prev` and propose `DeleteRight`, otherwise `CreateDirLeft`.

**Branch `(Some(l), Some(r))` (lines 266-273):** add a case for type mismatch (`l.is_dir != r.is_dir`) — produce `Conflict { reason: BothModified, … }`. This is a new surface guarantee: if the same path turns out to be a file on one side and a directory on the other, we don't silently skip or crash. (The existing `if l.is_dir && r.is_dir { Skip }` branch is already correct.)

### 4. Tests

In `src/sync/differ.rs` (the `tests` module) add:

- `test_deleted_dir_propagates_to_right`: left has empty `folder/`, right does not, both metadatas contain `folder/` with `is_dir = true` → expect `DeleteLeft`.
- `test_deleted_dir_propagates_to_left`: symmetric.
- `test_first_sync_dir_only_on_left_creates_right`: empty metadata, left has `folder/`, right does not → `CreateDirRight` (regression — first-sync behavior must not break).
- `test_type_change_creates_conflict`: file `x` on the left, directory `x` on the right → `Conflict`.

In `src/sync/metadata.rs` (the `tests` module): update existing test constructors and add a test that deserializing an old `state.json` (manually constructed JSON without `is_dir`) succeeds and yields `is_dir == false`.

## Files to modify

- `src/sync/metadata.rs` — add `is_dir` to `FileState` and `DeletedFile`, fix test helpers.
- `src/app/mod.rs` — `save_sync_metadata`: handle `CreateDirRight/Left`, propagate `is_dir` into `mark_deleted`.
- `src/sync/differ.rs` — directory logic in `determine_action`, plus tests in the existing `tests` module.

## Existing functions to reuse

- `SyncMetadata::upsert_file` (`metadata.rs:218`) — already removes the entry from `deleted` and adds/updates it in `files`. Fits one-to-one for recording a directory.
- `SyncMetadata::mark_deleted` (`metadata.rs:188`) — fits as is, only the new `is_dir` field needs to be filled in.
- `SyncMetadata::find_file` / `find_deleted` (`metadata.rs:208,213`) — already used by the differ, no extra lookups required.

## Verification

1. `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` — all unit/integration tests pass, including the new ones.
2. Manual check on Windows for the bug scenario:
   - Create a project `c:\tmp\sn-files` ↔ `c:\tmp\sn-files2`.
   - Create an empty `folder/` on the left, run sync — verify `c:\tmp\sn-files\.rahzom\state.json` and `c:\tmp\sn-files2\.rahzom\state.json`: both must contain a record for `folder` with `"is_dir": true`.
   - Delete `c:\tmp\sn-files\folder\`, run Analyze — Preview must show **`DeleteRight`** for `folder/` (arrow → ×, not ← copy).
   - Apply sync, verify that `c:\tmp\sn-files2\folder\` was moved to `.rahzom/_trash` (soft-delete by default).
3. Regression for a non-empty subtree: place `folder/a.txt`, sync, then delete the whole `folder/` on one side, run Analyze again. Expect two `DeleteRight` actions (for `folder/` and `folder/a.txt`), not `CreateDirLeft + DeleteRight`.
4. Compatibility: open a project whose `state.json` was created by an older version (without `is_dir`), confirm that the application starts and does not report a deserialization error.

## Intentionally out of scope

- A full migration / rewrite of `state.json` for directories that were synchronized before the upgrade: after the update, the application does not "see" old folders in metadata, so the very first deletion of an empty folder synchronized before the upgrade will still propose `CreateDir`. This is unfortunate but transparent — after the next sync the directory enters metadata and behavior becomes correct. A full "scan + register existing directories" step is a separate task.
- File-vs-directory type conflicts are reported as `Conflict::BothModified` — coarse-grained. A more nuanced variant (a dedicated enum case) is a separate task.
- Reordering `CreateDir` vs `Delete` in the executor (`executor.rs:282-298`). Currently `CreateDir` runs before `Delete`, which is what causes the "recreate + delete files" effect today when metadata is missing. After the differ fix this case simply will not arise, so we leave the order untouched.

---

## Follow-up to commit `d87d944`: directory tombstones + sync batch ordering

External review of the previous commit `d87d944` ("Track directories in metadata to propagate deletions") surfaced two issues. Both are confirmed in the code.

### Finding 1 — directory tombstones treated as delete authority (data-safety)

`src/sync/differ.rs:342` for the directory case does:

```rust
if right_deleted || right_prev.is_some() {
    return SyncAction::DeleteLeft { path: path_buf };
}
```

`right_deleted` is passed in as a `bool` (see the `right_deleted.is_some()` calls at `:212` and `:243`), so the `is_dir` of the tombstone is not even available to `determine_action`. `right_prev` is available as `Option<&FileState>`, but its `is_dir` is not checked. The same gap exists symmetrically in the `(None, Some)` / `left_*` branch.

Consequence: a path previously recorded as a file (in `files` or `deleted` with `is_dir=false`) that now exists on one side as a directory is mistakenly steered into `DeleteLeft`/`DeleteRight` instead of a conflict. With `soft_delete: true` (default) the new directory is moved to `.rahzom/_trash` (recoverable, but still silent loss of user content); with `soft_delete: false`, `fs::remove_dir` or `fs::remove_file` destroys fresh data. This is release-blocking for a fix that touches deletion semantics.

The same problem exists symmetrically in the file branch `(Some, None)`: if `right_prev` has `is_dir=true` (i.e. the path was previously synchronized as a directory), but the left side now holds a file at that path, the current logic collapses it into `DeleteLeft` or `Conflict::ModifiedAndDeleted`, whereas the correct outcome is also "type change → conflict".

### Finding 2 — batch ordering is bypassed in app sync

`src/app/mod.rs:418-437` (`process_sync_step`) takes **one** action per frame from `syncing.actions` and passes it to `Executor::execute` wrapped in `vec![action.clone()]`. `Executor::sort_actions` (`executor.rs:273-279`) only sorts what arrives in a single call — for a one-element vector it is a no-op. `syncing.actions` itself is built without sorting (`mod.rs:290-294` via `filter_map`), and the underlying `diff_result.actions` is produced by iterating two `HashMap`s, so order is non-deterministic.

Consequence: for a non-empty subtree deleted whole on one side, the differ emits actions for both the parent and the children; if execution order happens to put the parent `Delete*` before its child, then with `soft_delete: false` `fs::remove_dir(parent)` (`executor.rs:480`) fails with "directory not empty", the executor records a failed action, and after the next sync the empty directory is still around on the source side or, conversely, the child file ends up orphaned. The behavior is flaky because of `HashMap` iteration order. With `soft_delete: true` the parent rename into `.rahzom/_trash` carries the whole subtree at once, which masks the issue — but it breaks on the first hard-delete or in cross-volume situations.

Both findings can be fixed using the same machinery: sort the action vector once before sync starts, by the same key that `Executor::action_order` already computes (deletes last, deepest first).

### Goal

1. The differ stops auto-deleting a file/directory when the metadata record describes a different type of entity (file ↔ dir mismatch). Such cases are surfaced as `Conflict`.
2. The action queue in `SyncingState` is sorted exactly once before the first `process_sync_step`, so the global ordering "child deletes before parent deletes" is enforced even with step-by-step execution.
3. Existing scenarios (happy-path deletion of a synchronized directory, first sync, ordinary file delete) continue to work without regressions.

### Changes

#### 1. `src/sync/metadata.rs` — no changes

The `is_dir` field is already in place (commit `d87d944`).

#### 2. `src/sync/differ.rs` — honor `is_dir` in prev/deleted

Change the signature of `determine_action` (`:253-261`): `right_deleted: bool` / `left_deleted: bool` → `right_deleted: Option<&DeletedFile>` / `left_deleted: Option<&DeletedFile>`. The call sites (`:206-214` and `:236-244`) pass `right_meta.find_deleted(path)` directly.

Inside `determine_action`:

**Branch `(Some(l), None)`:**

```rust
let right_prev_dir   = right_prev.is_some_and(|f| f.is_dir);
let right_prev_file  = right_prev.is_some_and(|f| !f.is_dir);
let right_del_dir    = right_deleted.is_some_and(|d| d.is_dir);
let right_del_file   = right_deleted.is_some_and(|d| !d.is_dir);

if l.is_dir {
    // Type mismatch: history is for a file, but the left side now has a directory.
    if right_prev_file || right_del_file {
        return Conflict { reason: ExistsVsDeleted, left: Some(...), right: None };
    }
    // Directory tombstone + dir still on the left — mirror file ExistsVsDeleted.
    if right_del_dir {
        return Conflict { reason: ExistsVsDeleted, left: Some(...), right: None };
    }
    // Directory was previously synchronized, now gone on the right → propagate deletion.
    if right_prev_dir {
        return DeleteLeft { path };
    }
    return CreateDirRight { path };
}

// l is a file
if right_prev_dir || right_del_dir {
    // The path used to be a directory; now there is a file at that location on the left.
    return Conflict { reason: ExistsVsDeleted, left: Some(...), right: None };
}
// existing file logic below — unchanged
```

The **`(None, Some(r))`** branch is updated symmetrically via `left_prev_*` / `left_del_*`.

The **`(Some, Some)`** branch's `l.is_dir != r.is_dir` check already exists (commit `d87d944`) — left as is.

#### 3. `src/sync/executor.rs` — expose the batch sorter

`sort_actions` and `action_order` are currently `fn` methods on `Executor`, but they don't use any field of `self`. Promote `action_order` to a free `fn action_order(action: &SyncAction) -> (u8, usize)` (the third tuple element of type `bool` is dead — it's not a tie-breaker because the first element already discriminates the kind, drop it for clarity), and turn `sort_actions` into a free `pub fn sort_actions(actions: Vec<SyncAction>) -> Vec<SyncAction>`. Inside `Executor::execute` (`:235`) we keep calling `sort_actions(actions)` (defense-in-depth — in case a direct caller passes an unsorted batch).

Alternative considered (rejected): keep `sort_actions` as a method and add a public free shortcut `sort_sync_actions` next to it. Less elegant and `&self` really isn't needed; pick the first option.

#### 4. `src/app/mod.rs` — sort the batch once before start

In `start_sync`, after collecting `actions` (`:290-294`) and before storing them into `SyncingState` (`:380-389`), call the new `sort_actions`:

```rust
let actions: Vec<SyncAction> = preview
    .actions
    .iter()
    .filter_map(|ua| ua.to_sync_action())
    .collect();
let actions = crate::sync::executor::sort_actions(actions);
```

(The `executor` module is already imported in this file.) The duplicate sort inside `Executor::execute` stays — it's harmless and preserves the invariant for any other potential consumer.

#### 5. Tests

In `src/sync/differ.rs::tests`:
- `test_dir_with_file_tombstone_creates_conflict` — `right_meta.deleted` contains a record with `is_dir=false`, the left side has directory `x/`, the right side has nothing → `Conflict { reason: ExistsVsDeleted }`. **The main regression test for Finding 1.**
- `test_dir_with_file_prev_creates_conflict` — `right_meta.files` contains a file `x` with `is_dir=false`, the left side has directory `x/`, the right side has nothing → Conflict.
- `test_dir_with_dir_tombstone_creates_conflict` — `right_meta.deleted` contains a record with `is_dir=true`, the left side has directory `x/`, the right side has nothing → Conflict (mirror of the file ExistsVsDeleted case).
- `test_file_with_dir_prev_creates_conflict` — `right_meta.files` has a record with `is_dir=true`, the left side has a file, the right side has nothing → Conflict.
- `test_file_with_dir_tombstone_creates_conflict` — symmetric variant with a tombstone instead of a current record.
- Regression: existing `test_deleted_dir_propagates_to_right`, `test_deleted_dir_propagates_to_left`, `test_first_sync_dir_only_on_left_creates_right`, `test_type_change_creates_conflict`, `test_deleted_left_unchanged_right_deletes_right` must continue to pass.

In `src/sync/executor.rs::tests`:
- `test_sort_actions_orders_deletes_deepest_first` — on a mixed vector `[CreateDirLeft, CopyToRight, DeleteRight("a/b/c"), DeleteRight("a/b"), DeleteRight("a")]`, after `sort_actions` the deletes come after copies and dir creations, with `a/b/c` before `a/b` before `a`. Covers Finding 2.

### Files to modify

- `src/sync/differ.rs` — `determine_action` signature, two branches updated, new tests.
- `src/sync/executor.rs` — turn `sort_actions` / `action_order` into free functions, make `sort_actions` `pub`. Existing executor tests stay valid (internal call inside `Executor::execute` keeps working).
- `src/app/mod.rs` — call `sort_actions` after building `actions` in `start_sync`.

### Existing functions to reuse

- `SyncMetadata::find_deleted` (`metadata.rs:213`) — its return value is now passed through as `Option<&DeletedFile>` instead of `bool`.
- `Executor::action_order` logic — reused as the free `action_order`.
- `ConflictReason::ExistsVsDeleted` — reused for both new type-mismatch cases (alongside the existing tombstone-vs-existing-file case for files).

### Verification

1. `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` — all tests, including the new regressions, pass.
2. Manual check for Finding 1 on Windows:
   - Create a project, add `c:\tmp\sn-files\x.txt`, sync. Both sides' `state.json` have a record `x.txt` with `is_dir: false`.
   - Delete `x.txt` through rahzom (sync). `state.json` now contains a tombstone for `x.txt` with `is_dir: false`.
   - On the left side, create a **directory** `x/`. Run Analyze.
   - Expectation: Preview shows `Conflict` for `x` (not `DeleteLeft` / silent deletion of the new directory).
3. Manual check for Finding 2:
   - With `soft_delete: false` in settings, create a tree `c:\tmp\sn-files\folder\a.txt`, sync.
   - Delete the whole `folder\` on one side via Explorer. Run Analyze, apply sync.
   - Expectation: no empty `folder/` left on either side; `result.failed` contains no `directory not empty` actions.

### Bump version

`Cargo.toml`: `0.14.1` → `0.14.2` (bug fix without new features).

### Intentionally out of scope

- A dedicated `ConflictReason::TypeChanged` enum variant. We use `ExistsVsDeleted` as the closest match in spirit (the previous-type record has effectively "vanished", an entity of a different type appeared in its place). A proper new variant is a separate task.
- Cleanup of tombstones whose path is now occupied by an entity of a different type: those will continue to "linger" in `deleted` until the retention deadline. Not critical; also a separate task.
- A full integration test of app sync for Finding 2 (would require a scenario test on a real FS driving `process_sync_step` step by step). The unit test on `sort_actions` is enough to guarantee the order; `app/mod.rs::start_sync` itself is covered by manual verification.

---

## Second follow-up: stale `right_prev` after a propagated deletion

External review of the previous follow-up commit found one more data-safety hole, complementary to Finding 1.

### Finding 3 — recreated directory can still be auto-deleted

After the previous fix the directory branch in `src/sync/differ.rs` still treats `right_prev_dir == true` alone as authority to `DeleteLeft`. The pair `save_sync_metadata` writes is asymmetric:

- `SyncAction::DeleteLeft { path }` only tombstones `left_meta` (`src/app/mod.rs`); `right_meta` retains the stale `files[path]` entry from before the OS-level deletion that triggered the propagation.
- `SyncAction::DeleteRight { path }` is the mirror: only `right_meta` is tombstoned; `left_meta.files[path]` is left stale.

Reproducer:

1. `x/` exists on both sides; both metadatas record `files["x"]` with `is_dir=true`.
2. The user deletes `x/` on the right side outside rahzom.
3. Analyze proposes `DeleteLeft`. Sync applies it. `left_meta` now has a tombstone for `x` (`is_dir=true`); `right_meta` still has `files["x"]` (`is_dir=true`).
4. The user later creates a fresh directory `x/` on the left side.
5. Differ sees: left has `x/`, right has nothing, `right_prev_dir == true` (stale), `right_del_dir == false` (no tombstone on right) → returns `DeleteLeft`.
6. The newly created directory is silently destroyed on the next sync (with `soft_delete: true` it goes to `.rahzom/_trash`, with `soft_delete: false` it is removed).

The file branch is not vulnerable because `left_prev.is_none()` makes `left_changed = true`, which routes to `Conflict::ModifiedAndDeleted`. The directory branch had no analogous local-state guard, since "directory modified" is not a meaningful concept.

### Goal

1. After a propagated deletion both sides' metadata reflect that the path is gone — no stale `files` records remain on the side that lost the entity OS-level.
2. Even when only one side's metadata claims a previous directory record, the differ refuses to auto-delete the other side's freshly created directory; such ambiguity surfaces as `Conflict::ExistsVsDeleted`.
3. Existing regression tests (`test_deleted_dir_propagates_to_*`, `test_first_sync_dir_only_on_left_creates_right`, the type-mismatch tests) continue to pass.

### Changes

#### 1. `src/app/mod.rs` — symmetric tombstones on propagated delete

In `save_sync_metadata`, both `SyncAction::DeleteLeft` and `SyncAction::DeleteRight` branches must mirror the tombstone onto the **other** side too.

Reasoning: `DeleteLeft` is only proposed when the differ has already concluded the right side lost the entity (OS-level), so by the time sync completes, both sides physically lack it; the metadata should reflect the same. Symmetric for `DeleteRight`.

Pseudo-code:

```rust
SyncAction::DeleteLeft { path } => {
    let path_str = path.to_string_lossy().to_string();
    let was_dir = left_meta
        .find_file(&path_str)
        .or_else(|| right_meta.find_file(&path_str))
        .map(|f| f.is_dir)
        .unwrap_or(false);
    let make = || DeletedFile {
        path: path_str.clone(),
        size: 0,
        mtime: now,
        hash: None,
        deleted_at: now,
        is_dir: was_dir,
    };
    left_meta.mark_deleted(make());
    right_meta.mark_deleted(make());
}
```

Symmetric for `DeleteRight`.

`mark_deleted` already removes any existing `files` and `deleted` entries for the path before pushing the new tombstone (`metadata.rs:188-195`), so calling it on a side whose `files` entry is stale cleans it up exactly as we want.

#### 2. `src/sync/differ.rs` — local-state guard in the directory branch

Pass `left_meta.find_deleted(path)` into the `(Some, None)` call (`differ.rs:206-214`) — currently `None` is hard-coded. Symmetrically pass `right_meta.find_deleted(path)` into the `(None, Some)` call (`:236-244`).

In the `(Some(l), None)` directory branch, replace the unconditional `DeleteLeft` on `right_prev_dir` with a stricter rule:

```rust
if right_prev_dir {
    let left_prev_dir = left_prev.is_some_and(|f| f.is_dir);
    if left_prev_dir && left_deleted.is_none() {
        return SyncAction::DeleteLeft { path: path_buf };
    }
    return SyncAction::Conflict {
        path: path_buf,
        reason: ConflictReason::ExistsVsDeleted,
        left: Some(left_info()),
        right: None,
    };
}
```

`left_deleted.is_none()` covers any leftover tombstone (file or dir): if the local side ever recorded a deletion at the same path, the directory we see now was created after that deletion, so the right-side `prev` record is no longer authoritative.

Symmetrically in the `(None, Some(r))` directory branch with `left_prev_dir`/`right_deleted`.

The file branches stay untouched — `left_changed = left_prev.is_none() || …` already protects them.

#### 3. Tests

Add to `src/sync/differ.rs::tests`:
- `test_dir_recreated_after_propagated_delete_creates_conflict` — `left_meta.deleted` has tombstone with `is_dir=true`, `right_meta.files` has stale `is_dir=true` record, left has fresh `x/`, right has nothing → `Conflict::ExistsVsDeleted`. **Main regression test for Finding 3.**
- `test_dir_recreated_after_propagated_delete_creates_conflict_symmetric` — mirror direction.
- `test_dir_propagated_delete_requires_local_dir_record` — left has `x/`, right has nothing, only `right_meta.files["x"]` is set with `is_dir=true` (no record on the left side at all) → Conflict, not DeleteLeft.

Add to `src/app/mod.rs` (or a small new test if there is no test module yet — pick the existing one): a unit test for `save_sync_metadata` covering the new symmetric tombstone behavior. If creating a unit harness around `save_sync_metadata` is too involved (it depends on `current_project`), settle for an integration-style assertion via end-to-end manual verification only and rely on the differ tests above as the primary regression coverage.

#### 4. `Cargo.toml`

Bump `0.14.2` → `0.14.3` (bug fix).

### Verification

1. `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
2. Manual reproduction on Windows of the Finding 3 scenario: create `x/` on both sides via sync, delete on right via Explorer, sync (which propagates `DeleteLeft`), inspect both `state.json` files — both must contain a tombstone for `x`. Then create a fresh `x/` on the left and run Analyze; Preview must show `Conflict` for `x`, not `DeleteLeft`.
3. Regression: original Finding 1 happy path (delete empty dir on right via Explorer, expect `DeleteRight`/`DeleteLeft` propagation) still works.
4. Backward compatibility: open a project whose `state.json` was created by 0.14.2 (asymmetric tombstones), confirm the differ surfaces `Conflict` on the recreated-directory case rather than `DeleteLeft`.

### Intentionally out of scope

- Same caveats as the previous follow-up: no dedicated `ConflictReason::TypeChanged` variant, no automatic cleanup of `deleted` entries displaced by entities of a different type.
- A unit-level harness for `save_sync_metadata`. The function is deeply coupled to `App.current_project`; refactoring it for testability is a larger change. The differ-level test covers the user-visible regression.
