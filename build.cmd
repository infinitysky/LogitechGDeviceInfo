@echo off
setlocal EnableExtensions
cd /d "%~dp0"

rem 从资源管理器双击时，用户目录下的 cargo 不在 PATH 里。
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

where cargo >nul 2>&1
if errorlevel 1 (
    echo 未找到 cargo。
    echo 请先安装 Rust: https://rustup.rs
    echo 安装完成后重新运行本脚本。
    pause
    exit /b 1
)

echo 正在编译 release...
cargo build --release
if errorlevel 1 (
    echo.
    echo 编译失败。
    pause
    exit /b 1
)

set "EXE=%~dp0target\release\logitray.exe"
if not exist "%EXE%" (
    echo.
    echo 编译结束，但没有找到 exe:
    echo   %EXE%
    pause
    exit /b 1
)

echo.
echo 完成:
echo   %EXE%
echo.
if /I not "%~1"=="/nopause" pause
exit /b 0
