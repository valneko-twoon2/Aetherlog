@echo off
rem Writes the fictional demo logs into docs\demo-logs (see make_demo_logs.ps1).
cd /d "%~dp0.."
powershell -NoProfile -ExecutionPolicy Bypass -File tools\make_demo_logs.ps1 %*
pause
