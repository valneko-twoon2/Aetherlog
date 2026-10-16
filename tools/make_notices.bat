@echo off
rem Regenerates THIRD-PARTY-NOTICES.txt (see make_notices.ps1).
cd /d "%~dp0.."
powershell -NoProfile -ExecutionPolicy Bypass -File tools\make_notices.ps1 %*
pause
