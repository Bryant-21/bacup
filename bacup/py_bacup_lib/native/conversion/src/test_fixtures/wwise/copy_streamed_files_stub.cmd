@echo off
rem Test stub for CopyStreamedFiles.exe. Delegates to a PowerShell script.
rem Args passed through verbatim.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0copy_streamed_files_stub.ps1" %*
exit /b %ERRORLEVEL%
