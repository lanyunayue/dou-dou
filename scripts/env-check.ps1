# Environment self-check for 豆 Dou desktop client (Tauri 2 + Vue 3).
# Probe-based: no hard-coded drive letters, user names or VS Code install paths.
# Usage: powershell -ExecutionPolicy Bypass -File .\scripts\env-check.ps1

$ErrorActionPreference = "Continue"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
function Say([string]$s) { Write-Output $s }

$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root

# The host process may have started before user env vars were written to the
# registry. Probe the local toolchain so we do not report false FAILs.
if (-not $env:CARGO_HOME -and (Test-Path -LiteralPath "D:\dev\.cargo\bin\cargo.exe")) {
    $env:CARGO_HOME = "D:\dev\.cargo"
    $env:RUSTUP_HOME = "D:\dev\.rustup"
    $env:Path = "D:\dev\.cargo\bin;" + $env:Path
}

$script:bad = 0
function Check([string]$Name, [scriptblock]$Test, [string]$Hint) {
    try {
        $v = (& $Test 2>$null | Out-String).Trim()
        if ([string]::IsNullOrWhiteSpace($v)) { throw "empty" }
        Say ("[OK]   {0,-24} {1}" -f $Name, $v)
    } catch {
        Say ("[FAIL] {0,-24} {1}" -f $Name, $Hint)
        $script:bad++
    }
}

# --- Resolve VS Code CLI by probing, not by a fixed path ---
$codeCli = $null
$cmd = Get-Command code -ErrorAction SilentlyContinue
if ($cmd) { $codeCli = $cmd.Source }
if (-not $codeCli) {
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA "Programs\Microsoft VS Code\bin\code.cmd"),
        (Join-Path ${env:ProgramFiles} "Microsoft VS Code\bin\code.cmd"),
        "D:\VSCode\bin\code.cmd"
    )
    foreach ($c in $candidates) {
        if (Test-Path -LiteralPath $c) { $codeCli = $c; break }
    }
}

$extDir = Join-Path $env:USERPROFILE ".vscode\extensions"
function Get-ExtList {
    if ($codeCli) {
        $l = (& $codeCli --list-extensions 2>$null | Out-String)
        if ($l -and $l.Trim().Length -gt 0) { return $l }
    }
    if (Test-Path -LiteralPath $extDir) {
        return ((Get-ChildItem -LiteralPath $extDir -Directory | Select-Object -ExpandProperty Name) -join "`n")
    }
    return ""
}
$script:extList = Get-ExtList
function Has-Ext([string]$id) {
    if ($script:extList -match [regex]::Escape($id)) { "installed" } else { throw }
}

$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }

Say "=== dou app environment self-check ==="
Say ("root       : {0}" -f $root)
Say ("VS Code CLI: {0}" -f $(if ($codeCli) { $codeCli } else { "NOT FOUND" }))
Say ("cargo home : {0}" -f $cargoHome)
Say "----------------------------------------"

Check "rustup"           { rustup show active-toolchain }              "install rustup"
Check "rustc"            { rustc --version }                           "install Rust"
Check "cargo"            { cargo --version }                           "install Rust"
Check "rustfmt"          { rustfmt --version }                         "rustup component add rustfmt"
Check "clippy"           { cargo clippy --version }                    "rustup component add clippy"
Check "node"             { node --version }                            "install Node.js 20+"
Check "npm"              { npm --version }                             "install Node.js 20+"
Check "tauri CLI"        { npm exec -- tauri --version }               "npm install"
Check "VS Code"          {
    if (-not $codeCli) { throw }
    $exe = Join-Path (Split-Path -Parent (Split-Path -Parent $codeCli)) "Code.exe"
    if (Test-Path -LiteralPath $exe) { (Get-Item -LiteralPath $exe).VersionInfo.ProductVersion }
    else { $v = (& $codeCli --version 2>$null | Out-String).Trim(); if ($v) { $v.Split("`n")[0] } else { throw } }
} "install VS Code / add its bin to PATH"
Check "ext rust-analyzer" { Has-Ext "rust-lang.rust-analyzer" }        "code --install-extension rust-lang.rust-analyzer"
Check "ext volar"        { Has-Ext "vue.volar" }                        "code --install-extension Vue.volar"
Check "ext tauri"        { Has-Ext "tauri-apps.tauri-vscode" }          "code --install-extension tauri-apps.tauri-vscode"
Check "ext cpptools"     { Has-Ext "ms-vscode.cpptools" }               "code --install-extension ms-vscode.cpptools"
Check "MSVC toolset"     {
    # Prefer vswhere for the real install path: VS Build Tools honors --installPath
    # (e.g. D:\VSBuildTools), so scanning only Program Files produces false FAILs.
    # Also keep both Program Files roots to cover VS 2026 (18) instances that
    # older vswhere builds do not list.
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    $roots = @((Join-Path $env:ProgramFiles "Microsoft Visual Studio"), (Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio"))
    if (Test-Path -LiteralPath $vswhere) {
        foreach ($p in @(& $vswhere -all -products '*' -property installationPath 2>$null)) {
            if ($p -and (Test-Path -LiteralPath $p)) { $roots += $p }
        }
    }
    if ($env:DOU_VS_PATH -and (Test-Path -LiteralPath $env:DOU_VS_PATH)) { $roots += $env:DOU_VS_PATH }
    # Only VC\Tools\MSVC counts; VC\Redist\MSVC\v14x is a redistributable folder, not a toolset.
    $toolDirs = Get-ChildItem -LiteralPath ($roots | Select-Object -Unique) -Recurse -Directory -Filter "MSVC" -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match "VC\\Tools\\MSVC$" }
    $ver = $toolDirs | ForEach-Object {
        Get-ChildItem -LiteralPath $_.FullName -Directory -ErrorAction SilentlyContinue
    } | Sort-Object Name | Select-Object -Last 1
    if ($ver) { "$($ver.Name) @ $($ver.Parent.FullName)" } else { throw }
} "install VS C++ desktop workload (requires elevated VS Installer)"
Check "Windows SDK"      {
    $p = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\Lib"
    if (Test-Path -LiteralPath $p) { (Get-ChildItem -LiteralPath $p -Directory | Select-Object -Last 1).Name } else { throw }
} "install Windows SDK 10.0.26100+"
Check "MinGW (GNU)"      {
    $g = Get-Command x86_64-w64-mingw32-gcc -ErrorAction SilentlyContinue
    if ($g) { $g.Source }
    elseif (Test-Path -LiteralPath "D:\w64devkit\bin\x86_64-w64-mingw32-gcc.exe") { "D:\w64devkit\bin" }
    else { throw }
} "install w64devkit / add its bin to PATH"
Check "cargo cache"      {
    $p = Join-Path $cargoHome "registry\cache"
    if ((Test-Path -LiteralPath $p) -and (Get-ChildItem -LiteralPath $p -File -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1)) { "present" }
    else { throw }
} "run one networked cargo build/fetch to warm the cache"
Check "toolchain pinned" {
    $p = Join-Path $root "rust-toolchain.toml"
    if (Test-Path -LiteralPath $p) { (Select-String -Path $p -Pattern '^channel').Line } else { throw }
} "missing rust-toolchain.toml"
Check "Cargo.lock"       {
    if (Test-Path -LiteralPath (Join-Path $root "src-tauri\Cargo.lock")) { "present" } else { throw }
} "cargo generate-lockfile"
Check "node_modules"     {
    if (Test-Path -LiteralPath (Join-Path $root "node_modules")) { "present" } else { throw }
} "npm ci"
Check "vscode config"    {
    if (Test-Path -LiteralPath (Join-Path $root ".vscode\tasks.json")) { "present" } else { throw }
} "missing .vscode/tasks.json"

Say "----------------------------------------"
if ($script:bad -eq 0) {
    Say "RESULT: ALL OK - ready for manual coding."
    Say "Next : .\scripts\dev.ps1 check | .\scripts\dev.ps1 dev | .\scripts\dev.ps1 build"
} else {
    Say ("RESULT: {0} item(s) need attention." -f $script:bad)
}
exit $script:bad
