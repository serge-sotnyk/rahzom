# Test Data Presets

Pre-defined folder structures for common test scenarios.

## basic_sync

Simple sync scenario with shared and unique files.

```bash
docker exec rahzom-test rm -rf /test/left/* /test/right/*

# Shared file (identical)
docker exec rahzom-test sh -c 'echo "shared content" > /test/left/shared.txt'
docker exec rahzom-test sh -c 'echo "shared content" > /test/right/shared.txt'

# Left-only file
docker exec rahzom-test sh -c 'echo "left only content" > /test/left/left_only.txt'

# Right-only file
docker exec rahzom-test sh -c 'echo "right only content" > /test/right/right_only.txt'

# Subdirectory with file
docker exec rahzom-test mkdir -p /test/left/subdir
docker exec rahzom-test sh -c 'echo "nested file" > /test/left/subdir/nested.txt'
```

**Expected actions**: Copy left_only.txt to right, copy right_only.txt to left, copy subdir/ to right.

---

## conflict

Files modified on both sides (triggers conflict detection).

```bash
docker exec rahzom-test rm -rf /test/left/* /test/right/*
docker exec rahzom-test rm -rf /test/left/.rahzom /test/right/.rahzom

# Create initial state and metadata
docker exec rahzom-test sh -c 'echo "original" > /test/left/doc.txt'
docker exec rahzom-test sh -c 'echo "original" > /test/right/doc.txt'

# Simulate previous sync by creating metadata
docker exec rahzom-test mkdir -p /test/left/.rahzom /test/right/.rahzom
docker exec rahzom-test sh -c 'echo "{\"files\":[{\"path\":\"doc.txt\",\"size\":9,\"mtime\":\"2024-01-01T00:00:00Z\"}],\"deleted\":[]}"> /test/left/.rahzom/state.json'
docker exec rahzom-test sh -c 'echo "{\"files\":[{\"path\":\"doc.txt\",\"size\":9,\"mtime\":\"2024-01-01T00:00:00Z\"}],\"deleted\":[]}"> /test/right/.rahzom/state.json'

# Now modify both sides differently
docker exec rahzom-test sh -c 'echo "version A from left" > /test/left/doc.txt'
docker exec rahzom-test sh -c 'echo "version B from right" > /test/right/doc.txt'
```

**Expected**: Conflict shown for doc.txt, user must choose direction.

---

## unicode

Files with Unicode characters in names.

```bash
docker exec rahzom-test rm -rf /test/left/* /test/right/*

# Cyrillic
docker exec rahzom-test sh -c 'echo "Ukrainian content" > "/test/left/документ.txt"'

# Chinese
docker exec rahzom-test sh -c 'echo "Chinese content" > "/test/left/文档.txt"'

# Emoji (if filesystem supports)
docker exec rahzom-test sh -c 'echo "Emoji content" > "/test/left/notes_📝.txt"' 2>/dev/null || echo "Emoji not supported"

# Mixed
docker exec rahzom-test sh -c 'echo "Mixed" > "/test/left/файл_file_文件.txt"'
```

**Expected**: All files detected and can be synced.

---

## long_names

Files with long names to test display truncation.

```bash
docker exec rahzom-test rm -rf /test/left/* /test/right/*

# Long filename
docker exec rahzom-test sh -c 'echo "long name" > "/test/left/this_is_a_very_long_filename_that_might_cause_display_issues_in_the_terminal_interface.txt"'

# Long path
docker exec rahzom-test mkdir -p "/test/left/deeply/nested/directory/structure/for/testing/long/paths"
docker exec rahzom-test sh -c 'echo "deep file" > "/test/left/deeply/nested/directory/structure/for/testing/long/paths/file.txt"'
```

**Expected**: Names truncated in UI but operations work correctly.

---

## large_tree

Many files for performance testing.

```bash
docker exec rahzom-test rm -rf /test/left/* /test/right/*

# Create 100 directories with 10 files each (1000 files total)
docker exec rahzom-test sh -c '
for i in $(seq -w 1 100); do
    mkdir -p /test/left/dir_$i
    for j in $(seq -w 1 10); do
        echo "content $i-$j" > /test/left/dir_$i/file_$j.txt
    done
done
'
```

**Expected**: Analyze completes in reasonable time, UI remains responsive.

---

## deletion

Test deletion propagation.

```bash
docker exec rahzom-test rm -rf /test/left/* /test/right/*
docker exec rahzom-test rm -rf /test/left/.rahzom /test/right/.rahzom

# Initial sync state: both have the file
docker exec rahzom-test sh -c 'echo "will be deleted" > /test/left/to_delete.txt'
docker exec rahzom-test sh -c 'echo "will be deleted" > /test/right/to_delete.txt'
docker exec rahzom-test sh -c 'echo "will stay" > /test/left/keep.txt'
docker exec rahzom-test sh -c 'echo "will stay" > /test/right/keep.txt'

# Create metadata showing synced state
docker exec rahzom-test mkdir -p /test/left/.rahzom /test/right/.rahzom
docker exec rahzom-test sh -c 'echo "{\"files\":[{\"path\":\"to_delete.txt\",\"size\":16,\"mtime\":\"2024-01-01T00:00:00Z\"},{\"path\":\"keep.txt\",\"size\":10,\"mtime\":\"2024-01-01T00:00:00Z\"}],\"deleted\":[]}"> /test/left/.rahzom/state.json'
docker exec rahzom-test sh -c 'echo "{\"files\":[{\"path\":\"to_delete.txt\",\"size\":16,\"mtime\":\"2024-01-01T00:00:00Z\"},{\"path\":\"keep.txt\",\"size\":10,\"mtime\":\"2024-01-01T00:00:00Z\"}],\"deleted\":[]}"> /test/right/.rahzom/state.json'

# Delete file on left side only
docker exec rahzom-test rm /test/left/to_delete.txt
```

**Expected**: Analyze shows "delete to_delete.txt from right" action.

---

## empty_dirs

Test empty directory handling.

```bash
docker exec rahzom-test rm -rf /test/left/* /test/right/*

# Empty directories
docker exec rahzom-test mkdir -p /test/left/empty_dir
docker exec rahzom-test mkdir -p /test/left/dir_with_subdir/empty_subdir

# Directory with file
docker exec rahzom-test mkdir -p /test/left/dir_with_file
docker exec rahzom-test sh -c 'echo "content" > /test/left/dir_with_file/file.txt'
```

**Expected**: Empty directories synced along with non-empty ones.
