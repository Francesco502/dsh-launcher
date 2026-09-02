@echo off
setlocal EnableExtensions
if "%~1"=="" (
    echo Usage: dshctl.cmd start^|stop^|upgrade^|open
    exit /b 2
)
set "ACTION=%~1"
if not exist "%~dp0data\tmp" mkdir "%~dp0data\tmp" >nul 2>nul
set "DSH_LAUNCHER_OUTPUT=%~dp0data\tmp\dshctl-%RANDOM%-%RANDOM%.txt"
start "" /wait /b "%~dp0DSH-Launcher.exe" --action "%ACTION%"
set "CODE=%ERRORLEVEL%"
if exist "%DSH_LAUNCHER_OUTPUT%" (
    powershell.exe -NoLogo -NoProfile -NonInteractive -Command "$p=$env:DSH_LAUNCHER_OUTPUT; [Console]::Out.Write((Get-Content -Raw -Encoding UTF8 -LiteralPath $p))"
    del /q "%DSH_LAUNCHER_OUTPUT%" >nul 2>nul
)
endlocal & exit /b %CODE%
