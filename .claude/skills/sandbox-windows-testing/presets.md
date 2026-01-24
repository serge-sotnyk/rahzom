# Test Data Presets (Windows)

Pre-defined folder structures for common test scenarios.

## basic_sync

Simple sync scenario with shared and unique files.

```powershell
Remove-Item "C:\rahzom-test\left\*" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "C:\rahzom-test\right\*" -Recurse -Force -ErrorAction SilentlyContinue

# Shared file (identical)
Set-Content "C:\rahzom-test\left\shared.txt" -Value "shared content"
Set-Content "C:\rahzom-test\right\shared.txt" -Value "shared content"

# Left-only file
Set-Content "C:\rahzom-test\left\left_only.txt" -Value "left only content"

# Right-only file
Set-Content "C:\rahzom-test\right\right_only.txt" -Value "right only content"

# Subdirectory with file
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\left\subdir" | Out-Null
Set-Content "C:\rahzom-test\left\subdir\nested.txt" -Value "nested file"
```

**Expected actions**: Copy left_only.txt to right, copy right_only.txt to left, copy subdir/ to right.

---

## conflict

Files modified on both sides (triggers conflict detection).

```powershell
Remove-Item "C:\rahzom-test\left\*" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "C:\rahzom-test\right\*" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "C:\rahzom-test\left\.rahzom" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "C:\rahzom-test\right\.rahzom" -Recurse -Force -ErrorAction SilentlyContinue

# Create initial state and metadata
Set-Content "C:\rahzom-test\left\doc.txt" -Value "original"
Set-Content "C:\rahzom-test\right\doc.txt" -Value "original"

# Simulate previous sync by creating metadata
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\left\.rahzom" | Out-Null
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\right\.rahzom" | Out-Null
$stateJson = '{"files":[{"path":"doc.txt","size":9,"mtime":"2024-01-01T00:00:00Z"}],"deleted":[]}'
Set-Content "C:\rahzom-test\left\.rahzom\state.json" -Value $stateJson
Set-Content "C:\rahzom-test\right\.rahzom\state.json" -Value $stateJson

# Now modify both sides differently
Set-Content "C:\rahzom-test\left\doc.txt" -Value "version A from left"
Set-Content "C:\rahzom-test\right\doc.txt" -Value "version B from right"
```

**Expected**: Conflict shown for doc.txt, user must choose direction.

---

## unicode

Files with Unicode characters in names.

```powershell
Remove-Item "C:\rahzom-test\left\*" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "C:\rahzom-test\right\*" -Recurse -Force -ErrorAction SilentlyContinue

# Cyrillic
Set-Content "C:\rahzom-test\left\документ.txt" -Value "Ukrainian content" -Encoding UTF8

# Chinese
Set-Content "C:\rahzom-test\left\文档.txt" -Value "Chinese content" -Encoding UTF8

# Mixed
Set-Content "C:\rahzom-test\left\файл_file_文件.txt" -Value "Mixed" -Encoding UTF8
```

**Expected**: All files detected and can be synced.

---

## long_names

Files with long names to test display truncation.

```powershell
Remove-Item "C:\rahzom-test\left\*" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "C:\rahzom-test\right\*" -Recurse -Force -ErrorAction SilentlyContinue

# Long filename
Set-Content "C:\rahzom-test\left\this_is_a_very_long_filename_that_might_cause_display_issues_in_the_terminal_interface.txt" -Value "long name"

# Long path
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\left\deeply\nested\directory\structure\for\testing\long\paths" | Out-Null
Set-Content "C:\rahzom-test\left\deeply\nested\directory\structure\for\testing\long\paths\file.txt" -Value "deep file"
```

**Expected**: Names truncated in UI but operations work correctly.

---

## large_tree

Many files for performance testing.

```powershell
Remove-Item "C:\rahzom-test\left\*" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "C:\rahzom-test\right\*" -Recurse -Force -ErrorAction SilentlyContinue

# Create 50 directories with 5 files each (250 files total)
1..50 | ForEach-Object {
    $dir = "C:\rahzom-test\left\dir_$($_.ToString('D3'))"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    1..5 | ForEach-Object -Begin { $d = $dir } -Process {
        Set-Content "$d\file_$($_.ToString('D2')).txt" -Value "content $_"
    }
}
```

**Expected**: Analyze completes in reasonable time, UI remains responsive.

---

## deletion

Test deletion propagation.

```powershell
Remove-Item "C:\rahzom-test\left\*" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "C:\rahzom-test\right\*" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "C:\rahzom-test\left\.rahzom" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "C:\rahzom-test\right\.rahzom" -Recurse -Force -ErrorAction SilentlyContinue

# Initial sync state: both have the file
Set-Content "C:\rahzom-test\left\to_delete.txt" -Value "will be deleted"
Set-Content "C:\rahzom-test\right\to_delete.txt" -Value "will be deleted"
Set-Content "C:\rahzom-test\left\keep.txt" -Value "will stay"
Set-Content "C:\rahzom-test\right\keep.txt" -Value "will stay"

# Create metadata showing synced state
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\left\.rahzom" | Out-Null
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\right\.rahzom" | Out-Null
$stateJson = '{"files":[{"path":"to_delete.txt","size":16,"mtime":"2024-01-01T00:00:00Z"},{"path":"keep.txt","size":10,"mtime":"2024-01-01T00:00:00Z"}],"deleted":[]}'
Set-Content "C:\rahzom-test\left\.rahzom\state.json" -Value $stateJson
Set-Content "C:\rahzom-test\right\.rahzom\state.json" -Value $stateJson

# Delete file on left side only
Remove-Item "C:\rahzom-test\left\to_delete.txt"
```

**Expected**: Analyze shows "delete to_delete.txt from right" action.

---

## empty_dirs

Test empty directory handling.

```powershell
Remove-Item "C:\rahzom-test\left\*" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "C:\rahzom-test\right\*" -Recurse -Force -ErrorAction SilentlyContinue

# Empty directories
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\left\empty_dir" | Out-Null
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\left\dir_with_subdir\empty_subdir" | Out-Null

# Directory with file
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\left\dir_with_file" | Out-Null
Set-Content "C:\rahzom-test\left\dir_with_file\file.txt" -Value "content"
```

**Expected**: Empty directories synced along with non-empty ones.

---

## Verification Commands

After running preset, verify with:

```powershell
# List left folder
Get-ChildItem "C:\rahzom-test\left" -Recurse | Select-Object FullName

# List right folder
Get-ChildItem "C:\rahzom-test\right" -Recurse | Select-Object FullName

# Check file content
Get-Content "C:\rahzom-test\left\shared.txt"
```
