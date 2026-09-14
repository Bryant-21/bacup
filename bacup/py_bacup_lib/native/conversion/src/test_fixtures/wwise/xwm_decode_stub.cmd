@echo off
rem Test stub for xwmaencode.exe / ffmpeg xwm->wav decode: fabricates the
rem output by copying the input bytes. Args: <in> <out>.
copy /Y "%~1" "%~2" >nul
exit /b 0
