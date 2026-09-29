@echo off
setlocal EnableExtensions EnableDelayedExpansion
chcp 65001 >nul
title Nestbox
cd /d "%~dp0"

echo.
echo   Nestbox – virtuelle Maschinen einfach gemacht
echo   ─────────────────────────────────────────────
echo.

set "FEHLER=0"

rem Rust liegt nach der Installation oft noch nicht im PATH dieser Sitzung
if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

rem ── Node.js ──
where node >nul 2>nul
if errorlevel 1 (
  echo   [X] Node.js fehlt.
  echo       Installieren:  winget install OpenJS.NodeJS.LTS
  echo       oder von https://nodejs.org ^(LTS-Version^)
  set "FEHLER=1"
) else (
  for /f "delims=" %%v in ('node -v') do echo   [OK] Node.js %%v
)

rem ── Rust ──
where cargo >nul 2>nul
if errorlevel 1 (
  echo   [X] Rust fehlt.
  echo       Installieren:  winget install Rustlang.Rustup
  echo       oder von https://rustup.rs
  set "FEHLER=1"
) else (
  for /f "tokens=2" %%v in ('cargo -V') do echo   [OK] Rust %%v
)

rem ── Visual Studio Build Tools (C++) ──
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set "VSPATH="
if exist "%VSWHERE%" (
  for /f "usebackq delims=" %%p in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSPATH=%%p"
)
if defined VSPATH (
  echo   [OK] Visual Studio Build Tools ^(C++^)
) else (
  echo   [X] Visual Studio Build Tools mit "Desktopentwicklung mit C++" fehlen.
  echo       Installieren:  winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
  set "FEHLER=1"
)

rem ── WebView2 ──
set "WV2="
reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv >nul 2>nul && set "WV2=1"
reg query "HKCU\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv >nul 2>nul && set "WV2=1"
if exist "%ProgramFiles(x86)%\Microsoft\EdgeWebView\Application" set "WV2=1"
if defined WV2 (
  echo   [OK] WebView2
) else (
  echo   [X] Microsoft Edge WebView2 fehlt.
  echo       Installieren:  winget install Microsoft.EdgeWebView2Runtime
  set "FEHLER=1"
)

echo.
if "%FEHLER%"=="1" (
  echo   Bitte installiere die fehlenden Programme, öffne danach ein neues Fenster
  echo   und starte start.bat erneut.
  echo.
  pause
  exit /b 1
)

rem ── Abhängigkeiten ──
if not exist "node_modules\@tauri-apps\cli" (
  echo   Installiere Abhängigkeiten ^(npm install^) ...
  call npm install
  if errorlevel 1 (
    echo.
    echo   [X] npm install ist fehlgeschlagen. Prüfe deine Internetverbindung.
    pause
    exit /b 1
  )
) else (
  echo   Abhängigkeiten sind installiert. Prüfe auf Änderungen ...
  call npm install --no-audit --no-fund >nul
)

echo.
if /i "%~1"=="build" (
  echo   Erzeuge den Installer ^(npm run tauri build^) – das dauert einige Minuten ...
  call npm run tauri build
) else (
  echo   Starte Nestbox ^(npm run tauri dev^). Der erste Start dauert einige Minuten,
  echo   weil Rust alles einmal kompiliert. Danach geht es schnell.
  echo.
  call npm run tauri dev
)
if errorlevel 1 (
  echo.
  echo   [X] Nestbox konnte nicht gestartet werden. Die Meldungen oben helfen bei der Fehlersuche.
  pause
  exit /b 1
)
endlocal
