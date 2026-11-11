# 用 MinGW 工具链构建。dlltool 必须能通过 PATH 找到，否则 windows-sys 编译失败。
# 用法：  .\scripts\build.ps1 build        # 调试构建
#         .\scripts\build.ps1 build --release
#         .\scripts\build.ps1 run

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

Write-Output "[build] mingw = $bin"
Write-Output "[build] target-dir = E:/dou-target（规避中文路径）"

$cmd = if ($args.Count -gt 0) { $args[0] } else { "build" }
$rest = if ($args.Count -gt 1) { $args[1..($args.Count - 1)] } else { @() }

# 运行中的网关会锁住 exe，导致 "failed to remove dou-gateway.exe (os error 5)"
Get-Process -Name "dou-gateway" -ErrorAction SilentlyContinue | ForEach-Object {
    Write-Output "[build] 停止运行中的 dou-gateway (PID $($_.Id))，否则无法覆盖 exe"
    Stop-Process -Id $_.Id -Force
    Start-Sleep -Milliseconds 500
}

& cargo $cmd @rest
exit $LASTEXITCODE
