@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0generate_icons.ps1" %*
