@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0sync_agents.ps1" %*
