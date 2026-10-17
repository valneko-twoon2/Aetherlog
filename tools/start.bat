@echo off
cd /d "%~dp0.."
if exist "artifacts\Aethelog.exe" (
  start "" "artifacts\Aethelog.exe"
  exit /b 0
)
if exist "src-tauri\target\release\Aethelog.exe" (
  start "" "src-tauri\target\release\Aethelog.exe"
  exit /b 0
)
echo Aethelog.exe is not built yet. Run: powershell -File tools\publish.ps1
pause
exit /b 1
