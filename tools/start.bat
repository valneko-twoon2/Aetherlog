@echo off
cd /d "%~dp0.."
if exist "artifacts\Aetherlog.exe" (
  start "" "artifacts\Aetherlog.exe"
  exit /b 0
)
if exist "src-tauri\target\release\Aetherlog.exe" (
  start "" "src-tauri\target\release\Aetherlog.exe"
  exit /b 0
)
echo Aetherlog.exe is not built yet. Run: powershell -File tools\publish.ps1
pause
exit /b 1
