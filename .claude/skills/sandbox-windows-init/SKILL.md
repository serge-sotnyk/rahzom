---
name: sandbox-windows-init
description: Initialize Windows environment for rahzom TUI testing. Use when setting up test environment, before running TUI tests, or when rebuilding binaries.
---

# Windows Sandbox Initialization

## Step 1: Check if rahzom-tester user exists

```powershell
Get-LocalUser -Name "rahzom-tester" -ErrorAction SilentlyContinue
```

**If user does NOT exist**, ask the user to run these commands as Administrator:

```powershell
# Run in PowerShell as Administrator (one-time setup)
$Password = ConvertTo-SecureString "YourPassword123" -AsPlainText -Force
New-LocalUser -Name "rahzom-tester" -Password $Password -PasswordNeverExpires
New-Item -ItemType Directory -Force -Path "C:\rahzom-test"
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\left"
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\right"
New-Item -ItemType Directory -Force -Path "C:\rahzom-test\bin"
$acl = Get-Acl "C:\rahzom-test"
$rule = New-Object System.Security.AccessControl.FileSystemAccessRule("rahzom-tester", "FullControl", "ContainerInherit,ObjectInherit", "None", "Allow")
$acl.SetAccessRule($rule)
Set-Acl "C:\rahzom-test" $acl
```

## Step 2: Check if binaries exist

```powershell
Test-Path "C:\rahzom-test\bin\console-bridge.exe"
Test-Path "C:\rahzom-test\bin\rahzom.exe"
```

**If binaries do NOT exist**, build and deploy:

```powershell
.\.claude\skills\sandbox-windows-init\build.ps1
```

## Step 3: Rebuild (when code changes)

```powershell
# Rebuild rahzom only
cargo build --release
Copy-Item ".\target\release\rahzom.exe" "C:\rahzom-test\bin\" -Force

# Rebuild console-bridge only
.\.claude\skills\sandbox-windows-init\build.ps1 -BridgeOnly

# Rebuild both
.\.claude\skills\sandbox-windows-init\build.ps1
```

## Result

After successful init:
- `C:\rahzom-test\bin\console-bridge.exe` ready
- `C:\rahzom-test\bin\rahzom.exe` ready
- Test directories `left\` and `right\` exist
