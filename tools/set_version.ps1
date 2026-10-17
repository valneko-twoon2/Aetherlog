<#
.SYNOPSIS
  Sets the app version in every file that carries it.

.DESCRIPTION
  Updates tauri.conf.json, Cargo.toml, Cargo.lock, package.json and package-lock.json.
  Without -Version it shows the current version and asks for the new one.

  powershell -File tools\set_version.ps1 [-Version 1.2.3]
#>
param(
    # New version in MAJOR.MINOR.PATCH form, optionally with a suffix such as 1.2.3-beta.1.
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Read-Text($path) { [System.IO.File]::ReadAllText($path, $utf8) }
function Write-Text($path, $text) { [System.IO.File]::WriteAllText($path, $text, $utf8) }

# Replaces the first match of $pattern (group 1 = text before the version, group 2 = after).
function Set-First($rel, $pattern, $new) {
    $path = Join-Path $root $rel
    $text = Read-Text $path
    $re = New-Object System.Text.RegularExpressions.Regex($pattern, 'Multiline')
    if (-not $re.IsMatch($text)) { throw "No version found in $rel" }
    $out = $re.Replace($text, { param($m) $m.Groups[1].Value + $new + $m.Groups[2].Value }, 1)
    Write-Text $path $out
    Write-Host "  updated $rel"
}

$current = (Read-Text (Join-Path $root 'src-tauri\tauri.conf.json') |
    Select-String -Pattern '"version"\s*:\s*"([^"]+)"').Matches[0].Groups[1].Value
Write-Host "Current version: $current"

if (-not $Version) { $Version = Read-Host 'New version (e.g. 1.0.1)' }
$Version = $Version.Trim()
if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$') {
    throw "Not a valid version: '$Version' (use MAJOR.MINOR.PATCH, e.g. 1.2.3)"
}
if ($Version -eq $current) { Write-Host 'Already at that version.'; return }

Set-First 'src-tauri\tauri.conf.json' '(^\s*"version"\s*:\s*")[^"]+(")' $Version
Set-First 'src-tauri\Cargo.toml' '(^version\s*=\s*")[^"]+(")' $Version
Set-First 'src-tauri\Cargo.lock' '(^name = "aethelog"\r?\nversion = ")[^"]+(")' $Version
Set-First 'web\package.json' '(^\s*"version"\s*:\s*")[^"]+(")' $Version

# package-lock.json repeats the version for the root package.
$lock = Join-Path $root 'web\package-lock.json'
if (Test-Path $lock) {
    $text = Read-Text $lock
    $re = New-Object System.Text.RegularExpressions.Regex('(^\s*"version"\s*:\s*")[^"]+(")', 'Multiline')
    $count = 0
    $text = $re.Replace($text, { param($m) $script:count++; if ($script:count -le 2) { $m.Groups[1].Value + $Version + $m.Groups[2].Value } else { $m.Value } })
    Write-Text $lock $text
    Write-Host '  updated web\package-lock.json'
}

Write-Host "Version is now $Version. Run tools\publish.ps1 to build it." -ForegroundColor Green
