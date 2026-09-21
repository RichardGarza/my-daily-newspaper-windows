@echo off
rem Double-click me. I build "My Daily Newspaper" for YOU - your name on the
rem masthead, your initials on the icon - install it and open it.
rem The real work is in tools\build.ps1; this file only starts it.
title Build My Daily Newspaper
cd /d "%~dp0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\build.ps1" %*
if errorlevel 1 (
  echo.
  echo The build stopped. Details are in build.log in this folder.
  pause
)
