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

# Resolve MinGW by probing: DOU_MINGW_BIN -> w64devkit -> WinGet package.
$bin = $env:DOU_MINGW_BIN
if (-not $bin -or -not (Test-Path -LiteralPath (Join-Path $bin "dlltool.exe"))) {
    $candidates = @("D:\w64devkit\bin")
    $pkgRoot = Join-Path $env:LOCALAPPDATA "Microsoft\WinGet\Packages"
    $pkg = Get-ChildItem -LiteralPath $pkgRoot -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like "BrechtSanders.WinLibs.POSIX.UCRT_*" } |
        Select-Object -First 1
    if ($pkg) { $candidates += (Join-Path $pkg.FullName "mingw64\bin") }
    foreach ($c in $candidates) {
        if (Test-Path -LiteralPath (Join-Path $c "dlltool.exe")) { $bin = $c; break }
    }
}
if (-not $bin) { throw "MinGW not found. Set DOU_MINGW_BIN to a bin dir containing dlltool.exe, or install w64devkit." }

$cargoBin = if ($env:CARGO_HOME) { Join-Path $env:CARGO_HOME "bin" } else { Join-Path $env:USERPROFILE ".cargo\bin" }
$env:Path = "$bin;$cargoBin;" + $env:Path

$repo = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $repo

# ASCII-only target dir: a non-ASCII repo path breaks MinGW dlltool.
if (-not $env:DOU_TARGET_DIR) { $env:DOU_TARGET_DIR = "D:/dou-target" }
$env:CARGO_TARGET_DIR = $env:DOU_TARGET_DIR
# Only force offline when the cache is warm; a fresh machine must fetch crates.
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
$cache = Join-Path $cargoHome "registry\cache"
if (-not (Test-Path -LiteralPath $cache) -or -not (Get-ChildItem -LiteralPath $cache -File -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1)) {
    $env:CARGO_NET_OFFLINE = "false"
}

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
