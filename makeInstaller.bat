@echo off
REM Personal and Educational Free License (PEF License / PEFL)
REM Version 1.0 - 2026-05-26
REM Copyright (c) 2026 Rubens Rainelli
REM
REM Contact Details:
REM   Email: rubens@rainelli.it
REM   GitHub: https://github.com/RubensRainelli/
REM   Tel: (+39) 342 811 5882
REM
REM Personal and educational free use permitted.
REM Corporate, business or enterprise use prohibited without written authorization and fee.
REM The full license can be found in the LICENSE file.

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0platforms\windows\scripts\make_installer.ps1" %*
exit /b %ERRORLEVEL%
