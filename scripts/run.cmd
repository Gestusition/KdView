@echo off
setlocal
chcp 65001 >nul
title RuView Baslat

powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0run.ps1" %*
set "exitCode=%ERRORLEVEL%"

echo.
if not "%exitCode%"=="0" (
    echo [HATA] RuView baslatilamadi. Ayrintilar yukarida.
) else (
    echo RuView baslatildi. Bu pencereyi kapatabilirsiniz.
)
echo.
pause
exit /b %exitCode%
