# One-stop dev commands for 豆 Dou desktop client.
# Pure ASCII on purpose: Windows PowerShell 5.1 reads BOM-less UTF-8 as ANSI.
# Usage:
#   .\scripts\dev.ps1 check          # cargo check + vue-tsc typecheck
#   .\scripts\dev.ps1 build          # cargo build (Rust backend)
#   .\scripts\dev.ps1 dev            # npm run tauri dev (hot reload)
#   .\scripts\dev.ps1 release        # npm run tauri build
#   .\scripts\dev.ps1 ssh-test       # SSH loopback end-to-end selfcheck (10 cases)
#   .\scripts\dev.ps1 fmt            # cargo fmt + cargo clippy

param(
    [string]$Action = "check"
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

$root = Split-Path -Parent $PSScriptRoot
$tauri = Join-Path $root "src-tauri"
$loopback = "E:/思考/dou/tools/ssh-loopback"

# Keep offline: all crates are already vendored in the cargo cache.
if (-not $env:CARGO_NET_OFFLINE) { $env:CARGO_NET_OFFLINE = "true" }
$env:RUST_BACKTRACE = "1"

function Run([string]$Label, [string]$Cwd, [string]$Cmd) {
    Write-Output ""
    Write-Output ("=== {0} ===" -f $Label)
    Set-Location -LiteralPath $Cwd
    Invoke-Expression $Cmd
    if ($LASTEXITCODE -ne 0) { throw ("{0} failed (exit {1})" -f $Label, $LASTEXITCODE) }
}

switch ($Action) {
    "check" {
        Run "cargo check --offline" $tauri "cargo check --offline"
        Run "vue-tsc --noEmit"     $root  "npm run build"
    }
    "build"   { Run "cargo build --offline" $tauri "cargo build --offline" }
    "dev"     { Run "tauri dev"             $root  "npm run tauri dev" }
    "release" { Run "tauri build"           $root  "npm run tauri build" }
    "fmt" {
        Run "cargo fmt --check"  $tauri "cargo fmt --check"
        Run "cargo clippy"       $tauri "cargo clippy --offline -- -D warnings"
    }
    "ssh-test" {
        Run "SSH loopback selfcheck" $loopback "cargo run --offline --bin dou-ssh-selfcheck"
    }
    default {
        Write-Output "Unknown action: $Action"
        Write-Output "Valid: check | build | dev | release | ssh-test | fmt"
        exit 2
    }
}
Write-Output ""
Write-Output "DONE: $Action"
