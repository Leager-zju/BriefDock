@echo off
setlocal
rem Hongguo connection mode is controlled by its own saved setting.
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\run-shell-prototype.ps1" -LaunchOnly -ServicePackages "%~dp0target\service-exports\lazybuddy-hongguo-service\target\packages-browser-current"
set "preview_exit=%errorlevel%"
if not "%preview_exit%"=="0" (
    echo.
    echo Preview startup failed. See the error above.
    pause
)
exit /b %preview_exit%
