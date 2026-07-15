@echo off
REM Build B.A.C.U.P. as a folder distribution for systems where the single-EXE
REM build cannot extract or load its bundled runtime files.
REM Output: dist\BACUP\BACUP.exe plus its _internal runtime directory.
call "%~dp0build_bacup.bat" -OneDir %*
