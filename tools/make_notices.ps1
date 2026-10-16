<#
.SYNOPSIS
  Regenerates THIRD-PARTY-NOTICES.txt (licence texts of the Rust crates and npm packages in the exe).

.DESCRIPTION
  Needs cargo and Node.js; web\node_modules must exist (npm install --prefix web).
  The text is compiled into the exe (the "third-party licences" window) and copied next
  to it by tools\publish.ps1.

  powershell -File tools\make_notices.ps1
#>
$ErrorActionPreference = 'Stop'
node (Join-Path $PSScriptRoot 'make_notices.mjs')
if ($LASTEXITCODE -ne 0) { throw 'make_notices failed' }
