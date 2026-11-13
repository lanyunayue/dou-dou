# Environment self-check for 豆 Dou desktop client (Tauri 2 + Vue 3).
# Comments are ASCII-friendly; console text uses explicit WriteLine to avoid
# PowerShell 5.1 mixing stdout/stderr and swallowing newlines.
# Usage: powershell -ExecutionPolicy Bypass -File .\scripts\env-check.ps1

$ErrorActionPreference = "Continue"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
# Use the native output stream (Write-Output). Console::WriteLine("") gets its
# newline swallowed by the PowerShell 5.1 host, which glues lines together.
function Say([string]$s) { Write-Output $s }

$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root

$script:bad = 0
function Check([string]$Name, [scriptblock]$Test, [string]$Hint) {
    try {
        $v = (& $Test 2>$null | Out-String).Trim()
        if ([string]::IsNullOrWhiteSpace($v)) { throw "empty" }
        Say ("[OK]   {0,-22} {1}" -f $Name, $v)
    } catch {
        Say ("[FAIL] {0,-22} {1}" -f $Name, $Hint)
        $script:bad++
    }
}

Say "=== dou app environment self-check ==="
Say ("root: {0}" -f $root)
Say "----------------------------------------"

Check "rustup"           { rustup show active-toolchain }              "install rustup"
Check "rustc"            { rustc --version }                           "install Rust"
Check "cargo"            { cargo --version }                           "install Rust"
Check "node"             { node --version }                            "install Node.js 20+"
Check "npm"              { npm --version }                             "install Node.js 20+"
Check "tauri CLI"        { npm exec -- tauri --version }               "npm install"
Check "VS Code"          {
    $p = "D:\Microsoft VS Code\Code.exe"
    if (Test-Path $p) { (Get-Item $p).VersionInfo.ProductVersion }
    else { throw }
} "install VS Code"
Check "ext rust-analyzer" {
    $list = (& "D:\Microsoft VS Code\bin\code.cmd" --list-extensions 2>$null | Out-String)
    if ($list -match "rust-lang\.rust-analyzer") { "installed" } else { throw }
} "code --install-extension rust-lang.rust-analyzer"
Check "ext volar"        {
    $list = (& "D:\Microsoft VS Code\bin\code.cmd" --list-extensions 2>$null | Out-String)
    if ($list -match "vue\.volar") { "installed" } else { throw }
} "code --install-extension Vue.volar"
Check "ext tauri"        {
    $list = (& "D:\Microsoft VS Code\bin\code.cmd" --list-extensions 2>$null | Out-String)
    if ($list -match "tauri-apps\.tauri-vscode") { "installed" } else { throw }
} "code --install-extension tauri-apps.tauri-vscode"
Check "ext cpptools"     {
    $list = (& "D:\Microsoft VS Code\bin\code.cmd" --list-extensions 2>$null | Out-String)
    if ($list -match "ms-vscode\.cpptools") { "installed" } else { throw }
} "code --install-extension ms-vscode.cpptools"
Check "MSVC toolset"     {
    $p = "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\MSVC"
    if (Test-Path $p) { (Get-ChildItem $p -Directory | Select-Object -Last 1).Name }
    else { throw }
} "install VS 2026 C++ workload"
Check "Windows SDK"      {
    $p = "C:\Program Files (x86)\Windows Kits\10\Lib"
    if (Test-Path $p) { (Get-ChildItem $p -Directory | Select-Object -Last 1).Name }
    else { throw }
} "install Windows SDK"
Check "toolchain pinned" {
    $p = Join-Path $root "rust-toolchain.toml"
    if (Test-Path $p) { (Select-String -Path $p -Pattern '^channel').Line }
    else { throw }
} "missing rust-toolchain.toml"
Check "Cargo.lock"       {
    if (Test-Path (Join-Path $root "src-tauri\Cargo.lock")) { "present" } else { throw }
} "cargo generate-lockfile"
Check "node_modules"     {
    if (Test-Path (Join-Path $root "node_modules")) { "present" } else { throw }
} "npm install"
Check "vscode config"    {
    if (Test-Path (Join-Path $root ".vscode\tasks.json")) { "present" } else { throw }
} "missing .vscode/tasks.json"

Say "----------------------------------------"
if ($script:bad -eq 0) {
    Say "RESULT: ALL OK - ready for manual coding."
    Say "Next : .\scripts\dev.ps1 check | .\scripts\dev.ps1 dev | .\scripts\dev.ps1 build"
} else {
    Say ("RESULT: {0} item(s) need attention." -f $script:bad)
}
exit $script:bad
