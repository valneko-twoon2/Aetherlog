<#
.SYNOPSIS
  Retakes the README screenshots (docs\images\main.png, menu.png, battle.png).

.DESCRIPTION
  Starts the built exe on the demo logs (docs\demo-logs) with a scratch data folder, so no real
  character, name or path appears, then clicks through the tabs and the menu and saves each
  window as a PNG.  The pictures replace docs\images only when all three were taken.
  Needs Node.js 22 or newer.  Do not use the mouse or keyboard while it runs: the window is
  moved to the foreground and resized.

  The demo logs are copied to C:\Users\Public\Aethelog-demo-logs, which is the log location
  the menu picture shows, and removed afterwards.

  powershell -File tools\make_screenshots.ps1            # uses artifacts\Aethelog.exe
  powershell -File tools\make_screenshots.ps1 -Build     # builds first (tools\publish.ps1)
  powershell -File tools\make_screenshots.ps1 -Exe path\to\Aethelog.exe
#>
param(
    # Build and publish the exe first.
    [switch]$Build,
    # The exe to photograph; default is the published one, else the release build.
    [string]$Exe,
    # Where the pictures go.
    [string]$Out
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $Out) { $Out = Join-Path $root 'docs\images' }

if ($Build) {
    & (Join-Path $PSScriptRoot 'publish.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'publish failed' }
}
if (-not $Exe) {
    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'src-tauri\target' }
    $Exe = @((Join-Path $root 'artifacts\Aethelog.exe'), (Join-Path $targetDir 'release\Aethelog.exe')) |
        Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if (-not $Exe -or -not (Test-Path -LiteralPath $Exe)) { throw 'No Aethelog.exe found. Build it first (-Build).' }
$Exe = (Resolve-Path -LiteralPath $Exe).Path

$demo = Join-Path $root 'docs\demo-logs'
if (-not (Test-Path -LiteralPath $demo)) { throw 'docs\demo-logs is missing. Run tools\make_demo_logs.ps1.' }

$logs = 'C:\Users\Public\Aethelog-demo-logs'
$scratch = Join-Path ([IO.Path]::GetTempPath()) ("aethelog-shots-" + [Guid]::NewGuid().ToString('N'))
$data = Join-Path $scratch 'data'
$shots = Join-Path $scratch 'out'
try {
    New-Item -ItemType Directory -Force $data, $shots | Out-Null
    if (Test-Path -LiteralPath $logs) { Remove-Item -LiteralPath $logs -Recurse -Force }
    Copy-Item -LiteralPath $demo -Destination $logs -Recurse

    Write-Host "==> Taking screenshots with $Exe" -ForegroundColor Cyan
    node (Join-Path $PSScriptRoot 'make_screenshots.mjs') $Exe $logs $data $shots
    if ($LASTEXITCODE -ne 0) { throw 'make_screenshots failed' }

    $names = 'main', 'menu', 'battle'
    foreach ($n in $names) {
        if (-not (Test-Path -LiteralPath (Join-Path $shots "$n.png"))) { throw "$n.png was not taken" }
    }
    New-Item -ItemType Directory -Force $Out | Out-Null
    foreach ($n in $names) { Copy-Item -LiteralPath (Join-Path $shots "$n.png") -Destination (Join-Path $Out "$n.png") -Force }
    Write-Host "Saved to $Out. Check every picture before committing." -ForegroundColor Green
}
finally {
    if (Test-Path -LiteralPath $logs) { Remove-Item -LiteralPath $logs -Recurse -Force -ErrorAction SilentlyContinue }
    Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue
}
