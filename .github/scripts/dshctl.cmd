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
    powershell.exe -NoLogo -NoProfile -NonInteractive -Command "$p=$env:DSH_LAUNCHER_OUTPUT; $previous=[Console]::OutputEncoding; try { [Console]::OutputEncoding=New-Object Text.UTF8Encoding($false); [Console]::Out.Write((Get-Content -Raw -Encoding UTF8 -LiteralPath $p)) } finally { [Console]::OutputEncoding=$previous }"
    del /q "%DSH_LAUNCHER_OUTPUT%" >nul 2>nul
)
endlocal & exit /b %CODE%
