@echo off
REM MentorMinds Backend - Quick Install Script for Windows

echo ============================================================
echo MentorMinds Backend - Installation Script
echo ============================================================
echo.

echo [Step 1/3] Installing npm dependencies...
echo This may take a few minutes. Please wait...
echo.

call npm install

if %errorlevel% neq 0 (
    echo.
    echo ERROR: npm install failed!
    echo Please check your internet connection and try again.
    exit /b 1
)

echo.
echo [Step 2/3] Setting up environment variables...
if not exist .env (
    copy .env.example .env
    echo Created .env file from .env.example
) else (
    echo .env file already exists, skipping...
)

echo.
echo [Step 3/3] Verifying installation...
echo.

node -v
npm -v

echo.
echo ============================================================
echo Installation Complete!
echo ============================================================
echo.
echo Next steps:
echo 1. Edit .env file and update configuration (optional)
echo 2. Run 'npm run dev' to start development server
echo.
echo For more information, see QUICKSTART.md
echo ============================================================
echo.

pause
