@echo off
chcp 65001 >nul
rem Сборка Blockbuster.exe (нужен Rust: https://rustup.rs  — при установке выберите "x86_64-pc-windows-msvc"
rem и Visual Studio Build Tools с компонентом "Разработка классических приложений на C++")
where cargo >nul 2>nul
if errorlevel 1 (
  echo Не найден cargo. Установите Rust с https://rustup.rs и запустите этот файл снова.
  pause
  exit /b 1
)
cargo build --release
if errorlevel 1 (
  echo.
  echo Сборка не удалась. Пришлите текст ошибки выше.
  pause
  exit /b 1
)
if exist Blockbuster_portable rmdir /s /q Blockbuster_portable
mkdir Blockbuster_portable
copy /y target\release\Blockbuster.exe Blockbuster_portable\ >nul
xcopy /e /i /y logos Blockbuster_portable\logos >nul
echo.
echo Готово: папка Blockbuster_portable (Blockbuster.exe + logos). Положите рядом ffprobe.exe и ffmpeg.exe при желании.
pause
