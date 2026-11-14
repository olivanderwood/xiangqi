#!/usr/bin/env bash
# Download the board-recognition assets into recognize/ for local dev.
# (Pin lives in recognize/RECOGNIZE_VERSION.toml.)
#
#   ./tools/fetch-recognize.sh
#
# Downloads the 3 Xiangqi Arena ONNX models (MIT, dffge552) + the
# ONNX Runtime 1.31.0 library for THIS platform (Linux: .so, macOS:
# .dylib) into recognize/. The models + library are never committed
# to git (see .gitignore). Requires curl + tar + a sha256 tool
# (sha256sum, or shasum on macOS).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TOML="$ROOT/recognize/RECOGNIZE_VERSION.toml"
DIR="$ROOT/recognize"

toml() { # toml <key> — read a quoted string value from the pin file
    grep -m1 "^$1 =" "$TOML" | sed -E 's/^[^=]*= *"([^"]*)".*/\1/'
}

REPO="$(toml repo)"
ORT_VERSION="$(toml version)"
PHYSICAL="$(toml physical)"
DETECTOR="$(toml detector)"
CLASSIFIER="$(toml classifier)"
OCR="$(toml ocr)"
OCR_REPO="$(toml ocr_repo)"

OS="$(uname -s)"
case "$OS" in
    Linux)
        ORT_ARCHIVE="onnxruntime-linux-x64-${ORT_VERSION}.tgz"
        ORT_LIB="lib/libonnxruntime.so.${ORT_VERSION}"
        ORT_OUT="libonnxruntime.so"
        ;;
    Darwin)
        # 1.31.0 ships macOS arm64 only (no universal2 / no x64 archive
        # in this release); the .dylib path inside is the same layout.
        ORT_ARCHIVE="onnxruntime-osx-arm64-${ORT_VERSION}.tgz"
        ORT_LIB="lib/libonnxruntime.${ORT_VERSION}.dylib"
        ORT_OUT="libonnxruntime.dylib"
        ;;
    *) echo "fetch-recognize.sh: unsupported OS: $OS (Linux/macOS only; Windows: fetch-recognize.bat)" >&2; exit 1 ;;
esac

mkdir -p "$DIR"
cd "$DIR"

# --- The 3 ONNX models (Xiangqi Arena, MIT) ---
for model in "$PHYSICAL" "$DETECTOR" "$CLASSIFIER"; do
    if [ -f "$model" ]; then
        echo "Already have $model, skipping."
    else
        echo "Downloading $model ..."
        curl -fSL --retry 3 -o "$model" "https://raw.githubusercontent.com/${REPO}/master/${model}"
    fi
done

# --- The OCR voter model (PP-OCRv6-tiny rec, Apache-2.0, PaddlePaddle) ---
if [ -f "$OCR" ]; then
    echo "Already have $OCR, skipping."
else
    echo "Downloading $OCR ..."
    curl -fSL --retry 3 -o "$OCR" "https://huggingface.co/${OCR_REPO}/resolve/main/inference.onnx"
fi

# --- The ONNX Runtime library (official release) ---
if [ -f "$ORT_OUT" ]; then
    echo "Already have $ORT_OUT, skipping."
else
    echo "Downloading $ORT_ARCHIVE ..."
    curl -fSL --retry 3 -o "$ORT_ARCHIVE" "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/${ORT_ARCHIVE}"
    echo "Extracting $ORT_LIB ..."
    tar xzf "$ORT_ARCHIVE" "${ORT_ARCHIVE%.tgz}/$ORT_LIB"
    cp "${ORT_ARCHIVE%.tgz}/$ORT_LIB" "$ORT_OUT"
    rm -rf "${ORT_ARCHIVE%.tgz}" "$ORT_ARCHIVE"
fi

echo "recognize/ is ready:"
ls -lh "$DIR"
