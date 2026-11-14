@echo off
setlocal
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-and-run.ps1"
set "build_exit=%errorlevel%"
if not "%build_exit%"=="0" (
    echo.
    echo LazyBuddy build or startup failed. See the error above.
    pause
)
exit /b %build_exit%
