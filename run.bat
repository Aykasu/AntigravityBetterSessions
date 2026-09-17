@echo off
cd /d "%~dp0"
if exist "antigravity-bettersessions.exe" (
    start "" "antigravity-bettersessions.exe"
) else if exist "antigravity-sentinel.exe" (
    start "" "antigravity-sentinel.exe"
) else (
    cargo run --release
)
exit
