@echo off
setlocal EnableExtensions
if "%~1"=="" (
    echo Usage: dshctl.cmd start^|restart^|stop^|upgrade^|repair^|migrate^|launcher-update^|open^|data
    exit /b 2
)
set "ACTION=%~1"
"%~dp0DSH-Launcher.exe" --action "%ACTION%"
set "CODE=%ERRORLEVEL%"
endlocal & exit /b %CODE%
