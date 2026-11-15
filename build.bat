@echo off
setlocal
cd /d "%~dp0"
title Build PdfCraft

rem Double-click to build PdfCraft for Windows. The result is dist\pdfcraft.exe.

set "PATH=%PATH%;%USERPROFILE%\.cargo\bin"

where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust was not found.
  echo Install it from https://rustup.rs and run this file again.
  echo Windows also needs the Desktop development with C++ tools from Visual Studio Build Tools.
  echo.
  pause
  exit /b 1
)

echo Building PdfCraft in release mode.
echo The first build downloads and compiles everything and can take 10 to 20 minutes.
echo Later builds are much faster.
echo.

cargo build --release -p pdfcraft
if errorlevel 1 (
  echo.
  echo The build FAILED. Read the messages above.
  echo.
  pause
  exit /b 1
)

if not exist "%~dp0dist" mkdir "%~dp0dist"
copy /y "%~dp0target\release\pdfcraft.exe" "%~dp0dist\pdfcraft.exe" >nul
if errorlevel 1 (
  echo.
  echo The build worked but the exe could not be copied to dist.
  echo It is at target\release\pdfcraft.exe.
  echo.
  pause
  exit /b 1
)

echo.
echo Build finished: %~dp0dist\pdfcraft.exe
echo.
pause
