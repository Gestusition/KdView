@echo off
setlocal
chcp 65001 >nul
title RuView Wi-Fi Ayari

powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0configure-wifi.ps1" %*
set "exitCode=%ERRORLEVEL%"

echo.
if not "%exitCode%"=="0" (
    echo [HATA] Wi-Fi ayari tamamlanamadi. Ayrintilar yukarida.
) else (
    echo Wi-Fi ayari tamamlandi.
)
echo.
pause
exit /b %exitCode%
