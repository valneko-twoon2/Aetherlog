<#
.SYNOPSIS
  Builds Aetherlog in release mode and publishes it to artifacts.

.DESCRIPTION
  1. Builds the web front end (npm run build).
  2. Builds the Tauri app without an installer (tauri build --no-bundle).
  3. Copies Aetherlog.exe into <repo>\artifacts, leaving artifacts\userdata alone.

  Run it from anywhere:  powershell -File tools\publish.ps1
  Honors CARGO_TARGET_DIR when it is set (use an absolute path).
#>
param(
    # Skip "npm install" even when web\node_modules is missing.
    [switch]$SkipInstall
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$web = Join-Path $root 'web'
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'src-tauri\target' }
$built = Join-Path $targetDir 'release\Aetherlog.exe'
$out = Join-Path $root 'artifacts'

function Invoke-Step([string]$Title, [scriptblock]$Action) {
    Write-Host "==> $Title" -ForegroundColor Cyan
    & $Action
    if ($LASTEXITCODE -ne 0) { throw "$Title failed (exit code $LASTEXITCODE)" }
}

# A running copy (from the build folder or the published folder) locks its executable.
$running = Get-Process Aetherlog -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -eq $built -or $_.Path.StartsWith($out, [StringComparison]::OrdinalIgnoreCase) }
if ($running) { throw 'Aetherlog is running. Close it and run this script again.' }

Push-Location $web
try {
    if (-not $SkipInstall -and -not (Test-Path 'node_modules')) {
        Invoke-Step 'npm install' { npm install }
    }
    Invoke-Step 'npm run build' { npm run build }
}
finally { Pop-Location }

# The tauri CLI finds src-tauri in sub-folders, so it has to run from the repository root.
Push-Location $root
try {
    Invoke-Step 'tauri build --no-bundle' { & (Join-Path $web 'node_modules\.bin\tauri.cmd') build --no-bundle }
}
finally { Pop-Location }

if (-not (Test-Path $built)) { throw "Build output not found: $built" }

# Only the executable is replaced: $out\userdata (the log archive, names) must survive.
New-Item -ItemType Directory -Force $out | Out-Null
# Removing first makes the file name take the new capitalisation, as Windows keeps the old one on overwrite.
$target = Join-Path $out 'Aetherlog.exe'
Remove-Item $target -Force -ErrorAction SilentlyContinue
Copy-Item $built $target -Force

Write-Host "Published to $out" -ForegroundColor Green
Get-ChildItem $out | Format-Table Name, Length -AutoSize
