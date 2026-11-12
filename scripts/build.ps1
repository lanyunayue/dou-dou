# Build helper for the MinGW toolchain.
# Pure ASCII on purpose: Windows PowerShell 5.1 reads BOM-less UTF-8 as ANSI.
# Usage:
#   .\scripts\build.ps1 build
#   .\scripts\build.ps1 build --release
#   .\scripts\build.ps1 -Toolchain stable-x86_64-pc-windows-msvc test

param(
    [string]$Toolchain = "stable-x86_64-pc-windows-gnu"
)

$ErrorActionPreference = "Stop"

$pkgRoot = Join-Path $env:LOCALAPPDATA "Microsoft\WinGet\Packages"
$mingw = Get-ChildItem -LiteralPath $pkgRoot -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -like "BrechtSanders.WinLibs.POSIX.UCRT_*" } |
    Select-Object -First 1

if (-not $mingw) {
    throw "未找到 MinGW (BrechtSanders.WinLibs.POSIX.UCRT)。先执行：winget install --id BrechtSanders.WinLibs.POSIX.UCRT -e --scope user"
}

$bin = Join-Path $mingw.FullName "mingw64\bin"
if (-not (Test-Path -LiteralPath (Join-Path $bin "dlltool.exe"))) {
    throw "MinGW bin 目录缺少 dlltool.exe：$bin"
}

$env:Path = "$bin;$env:USERPROFILE\.cargo\bin;" + $env:Path

$repo = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $repo

# ASCII-only target dir: a non-ASCII repo path breaks MinGW dlltool.
if (-not $env:DOU_TARGET_DIR) { $env:DOU_TARGET_DIR = "E:/dou-target" }
$env:CARGO_TARGET_DIR = $env:DOU_TARGET_DIR
if (-not $env:CARGO_NET_OFFLINE) { $env:CARGO_NET_OFFLINE = "true" }

# UTF-8 console so Chinese audit text is not garbled by the OEM code page.
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

Write-Output "[build] toolchain = $Toolchain"
Write-Output "[build] mingw = $bin"
Write-Output "[build] target-dir = $env:CARGO_TARGET_DIR"

$cmd = if ($args.Count -gt 0) { $args[0] } else { "build" }
$rest = if ($args.Count -gt 1) { $args[1..($args.Count - 1)] } else { @() }

# 运行中的网关会锁住 exe，导致 "failed to remove dou-gateway.exe (os error 5)"
Get-Process -Name "dou-gateway" -ErrorAction SilentlyContinue | ForEach-Object {
    Write-Output "[build] 停止运行中的 dou-gateway (PID $($_.Id))，否则无法覆盖 exe"
    Stop-Process -Id $_.Id -Force
    Start-Sleep -Milliseconds 500
}

& cargo "+$Toolchain" $cmd @rest
exit $LASTEXITCODE
