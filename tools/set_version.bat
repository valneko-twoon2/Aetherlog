@echo off
rem Asks for a new version number and writes it into every file that carries it.
cd /d "%~dp0.."
powershell -NoProfile -ExecutionPolicy Bypass -File tools\set_version.ps1 %*
pause
