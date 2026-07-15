@echo off
REM Build B.A.C.U.P. - Bethesda Asset Converter Universal Platform.
REM The Tales From Appalachia companion payload is retained; generated game assets
REM for Tales, Legends of the Wasteland, and Fables of the North are never bundled.
REM Double-click this file, or run it from a terminal. Pass -OneDir for a folder
REM build instead of a single file:  build_bacup.bat -OneDir
setlocal
cd /d "%~dp0.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build_bacup.ps1" %*
echo.
pause
