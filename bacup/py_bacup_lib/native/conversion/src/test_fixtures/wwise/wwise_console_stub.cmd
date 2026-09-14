@echo off
rem Test stub for WwiseConsole.exe generate-soundbank. Delegates to a
rem PowerShell script for XML/JSON handling. Args passed through verbatim.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0wwise_console_stub.ps1" %*
exit /b %ERRORLEVEL%
