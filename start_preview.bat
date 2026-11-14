@echo off
setlocal
rem Only this launch loses inherited proxy variables; system proxy/TUN is unchanged.
set "HTTP_PROXY="
set "HTTPS_PROXY="
set "ALL_PROXY="
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\run-shell-prototype.ps1" -LaunchOnly -ServicePackages "%~dp0target\service-exports\lazybuddy-hongguo-service\target\packages-browser-current"
set "preview_exit=%errorlevel%"
if not "%preview_exit%"=="0" (
    echo.
    echo Preview startup failed. See the error above.
    pause
)
exit /b %preview_exit%
