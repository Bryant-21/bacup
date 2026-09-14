@echo off
rem Test stub for the ffmpeg PCM-conform step: fabricates the output by
rem copying the input bytes. Args: <in> <out>.
copy /Y "%~1" "%~2" >nul
exit /b 0
