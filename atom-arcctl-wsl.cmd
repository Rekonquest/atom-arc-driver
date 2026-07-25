@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\Invoke-AtomArcWsl.ps1" %*
exit /b %ERRORLEVEL%
