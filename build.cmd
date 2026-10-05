@echo off
setlocal EnableExtensions
cd /d "%~dp0"

rem Double-clicking from Explorer does not put the user cargo bin on PATH.
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

where cargo >nul 2>&1
if errorlevel 1 (
    echo cargo was not found.
    echo Install Rust first: https://rustup.rs
    echo Run this script again after installing.
    pause
    exit /b 1
)

rem Link the C runtime into the exe so nothing else has to sit beside it.
set "RUSTFLAGS=-C target-feature=+crt-static %RUSTFLAGS%"

echo Building a single-file release...
cargo build --release
if errorlevel 1 (
    echo.
    echo Build failed.
    pause
    exit /b 1
)

set "BUILT=%~dp0target\release\logitray.exe"
if not exist "%BUILT%" (
    echo.
    echo Build finished, but the exe was not found:
    echo   %BUILT%
    pause
    exit /b 1
)

set "EXE=%~dp0logitray.exe"
copy /Y "%BUILT%" "%EXE%" >nul
if errorlevel 1 (
    echo.
    echo Built the exe, but could not copy it here:
    echo   %EXE%
    echo Close LogiTray if it is running, then run this script again.
    echo The built file is still at:
    echo   %BUILT%
    pause
    exit /b 1
)

echo.
echo Done. Single file:
echo   %EXE%
echo.
if /I not "%~1"=="/nopause" pause
exit /b 0
