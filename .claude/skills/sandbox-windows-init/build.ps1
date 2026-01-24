# build.ps1 - Build and deploy console-bridge and rahzom
# Usage: .\build.ps1 [-BridgeOnly] [-RahzomOnly]

param(
    [switch]$BridgeOnly,
    [switch]$RahzomOnly
)

$ErrorActionPreference = "Stop"
$SkillDir = $PSScriptRoot
$RepoRoot = (Get-Item $SkillDir).Parent.Parent.Parent.FullName
$TargetBin = "C:\rahzom-test\bin"

Write-Host "=== rahzom Build Script ===" -ForegroundColor Cyan
Write-Host "Skill dir: $SkillDir" -ForegroundColor Gray
Write-Host "Repo root: $RepoRoot" -ForegroundColor Gray
Write-Host ""

# Ensure target directory exists
if (-not (Test-Path $TargetBin)) {
    New-Item -ItemType Directory -Path $TargetBin -Force | Out-Null
    Write-Host "Created: $TargetBin" -ForegroundColor Green
}

if (-not $RahzomOnly) {
    Write-Host "Building console-bridge..." -ForegroundColor Yellow
    Push-Location (Join-Path $SkillDir "console-bridge")
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "console-bridge build failed"
        }
        $src = Join-Path $SkillDir "console-bridge\target\release\console-bridge.exe"
        Copy-Item $src $TargetBin -Force
        Write-Host "  console-bridge.exe deployed to $TargetBin" -ForegroundColor Green
    }
    finally {
        Pop-Location
    }
}

if (-not $BridgeOnly) {
    Write-Host "Building rahzom..." -ForegroundColor Yellow
    Push-Location $RepoRoot
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "rahzom build failed"
        }
        $src = Join-Path $RepoRoot "target\release\rahzom.exe"
        Copy-Item $src $TargetBin -Force
        Write-Host "  rahzom.exe deployed to $TargetBin" -ForegroundColor Green
    }
    finally {
        Pop-Location
    }
}

Write-Host ""
Write-Host "Build complete!" -ForegroundColor Cyan
Write-Host ""
Write-Host "Verify with:" -ForegroundColor Gray
Write-Host "  $TargetBin\console-bridge.exe --help"
Write-Host "  $TargetBin\rahzom.exe --version"
