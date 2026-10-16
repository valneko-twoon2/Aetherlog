<#
.SYNOPSIS
  Previews the front end alone, in a browser, with no Rust build and no real logs.

.DESCRIPTION
  Starts the Vite dev server. Outside the Tauri window the app talks to a generated
  mock backend (web\src\mock.js: 150,000 fictional lines, a fake start-up load, slow
  large pages), so layout, filters, paging and the loading popup can be checked quickly.
  Changes to web\src reload instantly. Press Ctrl+C to stop.

  Run it from anywhere:  powershell -File tools\preview.ps1
#>
param(
    # Do not open the browser automatically.
    [switch]$NoOpen
)

$ErrorActionPreference = 'Stop'
$web = Join-Path (Split-Path -Parent $PSScriptRoot) 'web'

Push-Location $web
try {
    if (-not (Test-Path 'node_modules')) {
        Write-Host '==> npm install' -ForegroundColor Cyan
        npm install
        if ($LASTEXITCODE -ne 0) { throw 'npm install failed' }
    }
    Write-Host '==> Vite dev server (mock backend) on http://localhost:5173' -ForegroundColor Cyan
    if ($NoOpen) { npm run dev } else { npm run dev -- --open }
}
finally { Pop-Location }
