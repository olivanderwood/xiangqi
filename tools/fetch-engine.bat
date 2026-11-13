@echo off
REM Tải Pikafish 2026-09-06 (binary Windows + pikafish.nnue) vào engine\
REM Bản Command Prompt của tools\fetch-engine.sh — chạy trên Command
REM Prompt, từ bất kỳ đâu (script tự tìm thư mục repo). Cần: curl (có
REM sẵn trên Windows 10/11) và một trình giải nén 7z — thử theo thứ tự:
REM 7-Zip (7z.exe trong PATH hoặc ở C:\Program Files\7-Zip), rồi tar
REM có sẵn trên Windows 10/11 (bsdtar, đọc được .7z), rồi Python có
REM py7zr. Các giá trị ghim theo engine\PIKAFISH_VERSION.toml (bản
REM 2026-09-06, commit 4c17cee).
setlocal

set "ROOT=%~dp0.."
pushd "%ROOT%" || (echo Khong tim thay thu muc repo & exit /b 1)

set "VERSION=2026-09-06"
set "ARCHIVE=Pikafish.2026-09-06.7z"
set "URL=https://github.com/official-pikafish/Pikafish/releases/download/Pikafish-2026-09-06/%ARCHIVE%"
set "ARCHIVE_SHA256=41952bbfe2520faceb5902c69e6ab4845cc999841d2b49a95cc1be7867a25e5b"
set "NNUE_SHA256=7d13d73569a9b571ba0eb20cf1596247bc2a42738967e61afef6482b231e900e"
set "WIN_EXE=Pikafish-Windows-x86-64-universal.exe"
set "TMPDIR=%TEMP%\pikafish-fetch"

where curl >nul 2>&1 || (echo Thieu curl.exe - can Windows 10/11 tro len & popd & exit /b 1)

if not exist "%ARCHIVE%" (
  echo Dang tai %ARCHIVE% ...
  curl -fSL --retry 3 -o "%ARCHIVE%" "%URL%" || (echo Tai that bai & popd & exit /b 1)
) else (
  echo Da co %ARCHIVE%, bo qua buoc tai.
)

echo Kiem tra sha256 cua archive ...
certutil -hashfile "%ARCHIVE%" SHA256 | find /i "%ARCHIVE_SHA256%" >nul || (echo SHA256 CUA ARCHIVE KHONG KHOP, dung giai nen! & popd & exit /b 1)
echo Archive OK.

if exist "%TMPDIR%" rmdir /s /q "%TMPDIR%"
mkdir "%TMPDIR%"

set "SEVENZIP="
where 7z >nul 2>&1 && set "SEVENZIP=7z"
if not defined SEVENZIP if exist "%ProgramFiles%\7-Zip\7z.exe" set "SEVENZIP=%ProgramFiles%\7-Zip\7z.exe"
if not defined SEVENZIP if exist "%ProgramFiles(x86)%\7-Zip\7z.exe" set "SEVENZIP=%ProgramFiles(x86)%\7-Zip\7z.exe"

if defined SEVENZIP goto :extract_7z
where tar >nul 2>&1 && goto :extract_tar
python -c "import py7zr" >nul 2>&1 && goto :extract_py
echo Can 7-Zip [cai 7-Zip hoac them 7z.exe vao PATH], tar co san tren Windows 10/11, hoac Python co py7zr de giai nen.
popd
exit /b 1

:extract_7z
echo Giai nen bang 7z ...
"%SEVENZIP%" x -y -o"%TMPDIR%" "%ARCHIVE%" >nul || (echo Giai nen that bai & popd & exit /b 1)
goto :extracted

:extract_tar
echo Giai nen bang tar co san cua Windows ...
tar -xf "%ARCHIVE%" -C "%TMPDIR%" >nul || (echo Giai nen that bai & popd & exit /b 1)
goto :extracted

:extract_py
echo Giai nen bang py7zr ...
python -m py7zr x "%ARCHIVE%" "%TMPDIR%" >nul || (echo Giai nen that bai & popd & exit /b 1)

:extracted
if not exist engine mkdir engine
for %%F in (%WIN_EXE% pikafish.nnue Copying.txt NNUE-License.md) do (
  if not exist "%TMPDIR%\%%F" (echo Thieu %%F trong archive & popd & exit /b 1)
  copy /y "%TMPDIR%\%%F" "engine\" >nul
)

echo Kiem tra sha256 cua pikafish.nnue ...
certutil -hashfile "engine\pikafish.nnue" SHA256 | find /i "%NNUE_SHA256%" >nul || (echo SHA256 CUA NNUE KHONG KHOP! & popd & exit /b 1)
echo NNUE OK.

echo Pikafish %VERSION% > "engine\SOURCE.txt"
echo Source: https://github.com/official-pikafish/Pikafish >> "engine\SOURCE.txt"
echo Tag: Pikafish-%VERSION% >> "engine\SOURCE.txt"
echo Commit: 4c17cee11f888ae1d48a9494f2e2239f019f0a1f >> "engine\SOURCE.txt"
echo Release archive: %URL% >> "engine\SOURCE.txt"
echo Archive sha256: %ARCHIVE_SHA256% >> "engine\SOURCE.txt"
echo NNUE sha256: %NNUE_SHA256% - tu tinh tu file giai nen, khong co hash chinh thuc tu upstream. >> "engine\SOURCE.txt"
echo License: engine GPLv3 - Copying.txt; pikafish.nnue NNUE-License, chi dung phi thuong mai. >> "engine\SOURCE.txt"

rmdir /s /q "%TMPDIR%"
del "%ARCHIVE%"
echo Xong: engine\%WIN_EXE% + engine\pikafish.nnue - Pikafish %VERSION%
popd
exit /b 0
