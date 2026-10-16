@echo off
rem Previews the front end in a browser with a mock backend (see preview.ps1).
cd /d "%~dp0.."
powershell -NoProfile -ExecutionPolicy Bypass -File tools\preview.ps1 %*
pause
