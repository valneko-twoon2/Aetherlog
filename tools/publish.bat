@echo off
rem Builds the app and puts Aetherlog.exe in the artifacts folder (see publish.ps1).
cd /d "%~dp0.."
powershell -NoProfile -ExecutionPolicy Bypass -File tools\publish.ps1 %*
pause
