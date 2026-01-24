# setup-user.ps1 - Create rahzom-tester user (run as Administrator)
# Usage: .\setup-user.ps1

#Requires -RunAsAdministrator

$ErrorActionPreference = "Stop"

$UserName = "rahzom-tester"
$HomePath = "C:\rahzom-test"

Write-Host "=== rahzom Test User Setup ===" -ForegroundColor Cyan
Write-Host ""

# Check if user already exists
if (Get-LocalUser -Name $UserName -ErrorAction SilentlyContinue) {
    Write-Host "User '$UserName' already exists." -ForegroundColor Yellow
} else {
    # Prompt for password
    $Password = Read-Host -AsSecureString "Enter password for $UserName"

    # Create user
    New-LocalUser -Name $UserName -Password $Password -Description "rahzom TUI test user" -PasswordNeverExpires
    Write-Host "User '$UserName' created." -ForegroundColor Green
}

# Create home directory
if (-not (Test-Path $HomePath)) {
    New-Item -ItemType Directory -Path $HomePath -Force | Out-Null
    Write-Host "Created directory: $HomePath" -ForegroundColor Green
} else {
    Write-Host "Directory already exists: $HomePath" -ForegroundColor Yellow
}

# Set permissions
$acl = Get-Acl $HomePath
$rule = New-Object System.Security.AccessControl.FileSystemAccessRule(
    $UserName,
    "FullControl",
    "ContainerInherit,ObjectInherit",
    "None",
    "Allow"
)
$acl.SetAccessRule($rule)
Set-Acl $HomePath $acl
Write-Host "Permissions set for '$UserName' on '$HomePath'" -ForegroundColor Green

# Create subdirectories
$subdirs = @("left", "right", "bin", ".rahzom")
foreach ($subdir in $subdirs) {
    $path = Join-Path $HomePath $subdir
    if (-not (Test-Path $path)) {
        New-Item -ItemType Directory -Path $path -Force | Out-Null
    }
}
Write-Host "Created subdirectories: $($subdirs -join ', ')" -ForegroundColor Green

Write-Host ""
Write-Host "Setup complete!" -ForegroundColor Cyan
Write-Host "Next step: Run build.ps1 to build and deploy binaries"
