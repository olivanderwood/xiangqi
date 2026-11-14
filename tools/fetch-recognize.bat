@echo off
REM Tai cac asset nhan dien ban co (3 model ONNX Xiangqi Arena + thu vien
REM ONNX Runtime 1.31.0) vao recognize\
REM Ban Command Prompt cua tools\fetch-recognize.sh — chay tren Command
REM Prompt, tu bat ky dau (script tu tim thu muc repo). Can: curl (co
REM san tren Windows 10/11) va tar co san (doc duoc .zip). Cac gia tri
REM ghim theo recognize\RECOGNIZE_VERSION.toml.
setlocal

set "ROOT=%~dp0.."
pushd "%ROOT%" || (echo Khong tim thay thu muc repo & exit /b 1)

set "REPO=dffge552/xiangqi-pwa-offline"
set "ORT_VERSION=1.31.0"
set "PHYSICAL=Entity_chess_recognition_model.onnx"
set "DETECTOR=online_xiangqi_piece_detector.onnx"
set "CLASSIFIER=online_xiangqi_classifier.onnx"
set "ORT_ARCHIVE=onnxruntime-win-x64-%ORT_VERSION%.zip"
set "ORT_LIB=onnxruntime-win-x64-%ORT_VERSION%\lib\onnxruntime.dll"
set "ORT_OUT=onnxruntime.dll"

where curl >nul 2>&1 || (echo Thieu curl.exe - can Windows 10/11 tro len & popd & exit /b 1)
where tar >nul 2>&1 || (echo Thieu tar.exe - can Windows 10/11 tro len & popd & exit /b 1)

if not exist "recognize" mkdir "recognize"
pushd "recognize" || (echo Khong vao duoc recognize & popd & exit /b 1)

for %%M in (%PHYSICAL% %DETECTOR% %CLASSIFIER%) do (
  if exist "%%M" (
    echo Da co %%M, bo qua.
  ) else (
    echo Dang tai %%M ...
    curl -fSL --retry 3 -o "%%M" "https://raw.githubusercontent.com/%REPO%/master/%%M" || (echo Tai %%M that bai & popd & popd & exit /b 1)
  )
)

if exist "%ORT_OUT%" (
  echo Da co %ORT_OUT%, bo qua.
) else (
  echo Dang tai %ORT_ARCHIVE% ...
  curl -fSL --retry 3 -o "%ORT_ARCHIVE%" "https://github.com/microsoft/onnxruntime/releases/download/v%ORT_VERSION%/%ORT_ARCHIVE%" || (echo Tai ORT that bai & popd & popd & exit /b 1)
  echo Giai nen %ORT_LIB% ...
  tar xzf "%ORT_ARCHIVE%" "%ORT_LIB%" || (echo Giai nen that bai & popd & popd & exit /b 1)
  copy /y "%ORT_LIB%" "%ORT_OUT%" >nul
  rmdir /s /q "onnxruntime-win-x64-%ORT_VERSION%"
  del "%ORT_ARCHIVE%"
)

echo recognize/ da san sang:
dir
popd
popd
endlocal
