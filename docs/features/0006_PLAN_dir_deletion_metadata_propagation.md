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
