# Quality gate: build + test + clippy, one command.
# Pure ASCII on purpose: Windows PowerShell 5.1 reads BOM-less UTF-8 as ANSI.
# Usage:
#   .\scripts\gate.ps1                         # GNU toolchain (default)
#   .\scripts\gate.ps1 -Toolchain stable-x86_64-pc-windows-msvc
#   .\scripts\gate.ps1 -SkipClippy

param(
    [string]$Toolchain = "stable-x86_64-pc-windows-gnu",
    [switch]$SkipClippy
)

$ErrorActionPreference = "Stop"

# --- 1. Resolve MinGW (auto-detect, no user-specific absolute path) ---
$pkgRoot = Join-Path $env:LOCALAPPDATA "Microsoft\WinGet\Packages"
$mingw = Get-ChildItem -LiteralPath $pkgRoot -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -like "BrechtSanders.WinLibs.POSIX.UCRT_*" } |
    Select-Object -First 1
if ($mingw) { $env:Path = (Join-Path $mingw.FullName "mingw64\bin") + ";" + $env:Path }
$cargoBin = if ($env:CARGO_HOME) { Join-Path $env:CARGO_HOME "bin" } else { Join-Path $env:USERPROFILE ".cargo\bin" }
$env:Path = $cargoBin + ";" + $env:Path

$repo = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $repo

# --- 2. ASCII-only target dir (Chinese path breaks MinGW dlltool) ---
if (-not $env:DOU_TARGET_DIR) { $env:DOU_TARGET_DIR = "D:/dou-target" }
$env:CARGO_TARGET_DIR = $env:DOU_TARGET_DIR

# UTF-8 output for Windows console (GBK otherwise garbles Chinese audit text)
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

# --- 3. Stop a running gateway so the exe can be replaced ---
Get-Process -Name "dou-gateway" -ErrorAction SilentlyContinue | ForEach-Object {
    Write-Output "[gate] stopping running dou-gateway (PID $($_.Id))"
    Stop-Process -Id $_.Id -Force
    Start-Sleep -Milliseconds 300
}

$failed = @()

Write-Output "=== [1/3] build ($Toolchain) ==="
& cargo "+$Toolchain" build
if ($LASTEXITCODE -ne 0) { $failed += "build" }

Write-Output "=== [2/3] test ($Toolchain) ==="
& cargo "+$Toolchain" test
if ($LASTEXITCODE -ne 0) { $failed += "test" }

if (-not $SkipClippy) {
    Write-Output "=== [3/3] clippy ($Toolchain) ==="
    & cargo "+$Toolchain" clippy --all-targets
    if ($LASTEXITCODE -ne 0) { $failed += "clippy" }
}

Write-Output ""
if ($failed.Count -gt 0) {
    Write-Output "GATE FAILED: $($failed -join ', ')"
    exit 1
}
Write-Output "GATE PASSED: build + test + clippy all green ($Toolchain)"
exit 0
