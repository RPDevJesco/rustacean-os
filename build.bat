@echo off
REM Rustacean OS Docker Build Script for Windows
REM
REM Usage: build.bat [options]
REM
REM Options:
REM   --no-cache    Force rebuild without Docker cache
REM   --shell       Open a shell in the build container

setlocal enabledelayedexpansion

set IMAGE_NAME=rustacean-builder
set OUTPUT_DIR=%~dp0output
set NO_CACHE=
set SHELL_MODE=

REM Parse arguments
:parse_args
if "%~1"=="" goto :done_parsing
if "%~1"=="--no-cache" set NO_CACHE=--no-cache
if "%~1"=="--shell" set SHELL_MODE=yes
if "%~1"=="--help" goto :show_help
if "%~1"=="-h" goto :show_help
shift
goto :parse_args

:show_help
echo Rustacean OS Docker Build Script for Windows
echo.
echo Usage: build.bat [options]
echo.
echo Options:
echo   --no-cache    Force rebuild without Docker cache
echo   --shell       Open a shell in the build container
echo   --help, -h    Show this help message
exit /b 0

:done_parsing

REM Create output directory
if not exist "%OUTPUT_DIR%" mkdir "%OUTPUT_DIR%"

echo ========================================
echo   Rustacean OS Docker Builder
echo ========================================
echo.

REM Build Docker image
echo [Docker] Building image '%IMAGE_NAME%'...
docker build %NO_CACHE% -t %IMAGE_NAME% .
if errorlevel 1 (
    echo [Error] Docker build failed!
    exit /b 1
)

if "%SHELL_MODE%"=="yes" (
    echo.
    echo [Docker] Opening shell in container...
    docker run --rm -it -v "%OUTPUT_DIR%:/output" %IMAGE_NAME% /bin/bash
    exit /b 0
)

echo.
echo [Docker] Running build...
docker run --rm -v "%OUTPUT_DIR%:/output" %IMAGE_NAME%
if errorlevel 1 (
    echo [Error] Build failed!
    exit /b 1
)

echo.
echo ========================================
echo   Output Files
echo ========================================
dir "%OUTPUT_DIR%"

echo.
echo Done! Output files are in: %OUTPUT_DIR%
