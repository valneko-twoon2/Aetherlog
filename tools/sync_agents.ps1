<#
.SYNOPSIS
Copies shared agent guidance from the selected tree to its mirror.
#>
param(
    [ValidateSet('claude', 'agents')]
    [string]$Source = 'claude'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$from = Join-Path $root ".$Source"
$other = if ($Source -eq 'claude') { 'agents' } else { 'claude' }
$to = Join-Path $root ".$other"
if (-not (Test-Path -LiteralPath $from -PathType Container)) {
    throw "Source tree does not exist: $from"
}
New-Item -ItemType Directory -Force -Path $to | Out-Null
# Refuse removals: reconcile deleted or renamed files explicitly before copying.
$sourceFiles = @(Get-ChildItem -LiteralPath $from -Recurse -File |
    Where-Object { $_.FullName -notmatch '[\\/]__pycache__[\\/]' })
$destFiles = @(Get-ChildItem -LiteralPath $to -Recurse -File |
    Where-Object { $_.FullName -notmatch '[\\/]__pycache__[\\/]' })
foreach ($file in $destFiles) {
    $relative = $file.FullName.Substring($to.Length + 1)
    if (-not (Test-Path -LiteralPath (Join-Path $from $relative))) {
        throw "Destination-only file: $relative. Reconcile it before synchronizing."
    }
}
foreach ($file in $sourceFiles) {
    $relative = $file.FullName.Substring($from.Length + 1)
    $target = Join-Path $to $relative
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
    Copy-Item -LiteralPath $file.FullName -Destination $target -Force
}
Write-Host "Synchronized .$Source to .$other"
