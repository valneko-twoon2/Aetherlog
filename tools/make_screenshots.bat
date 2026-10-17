@echo off
rem Retakes the README screenshots (see make_screenshots.ps1). Add -Build to build the exe first.
cd /d "%~dp0.."
powershell -NoProfile -ExecutionPolicy Bypass -File tools\make_screenshots.ps1 %*
pause
