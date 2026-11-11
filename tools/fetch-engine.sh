#!/usr/bin/env bash
# Download the pinned Pikafish release into engine/ for local dev.
# (Integration spec §8.3; pin lives in engine/PIKAFISH_VERSION.toml.)
#
#   ./tools/fetch-engine.sh
#
# Downloads the release archive, verifies its sha256, extracts the
# binary for THIS platform (Linux: Pikafish-Linux-x86-64-universal;
# macOS: Pikafish-MacOS-universal) + pikafish.nnue + licenses into
# engine/, verifies the NNUE sha256, and writes engine/SOURCE.txt. The
# NNUE file is never committed to git (see .gitignore). Requires curl +
# a sha256 tool (sha256sum, or shasum on macOS) and a 7z extractor
# (7z, 7zz, bsdtar or python3 py7zr).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TOML="$ROOT/engine/PIKAFISH_VERSION.toml"
ENGINE_DIR="$ROOT/engine"

toml() { # toml <key> — read a quoted string value from the pin file
    grep -m1 "^$1 =" "$TOML" | sed -E 's/^[^=]*= *"([^"]*)".*/\1/'
}

VERSION="$(toml version)"
SOURCE_URL="$(toml source_url)"
ARCHIVE_SHA256="$(toml archive_sha256)"
NNUE_SHA256="$(toml nnue_sha256)"
# The binary for THIS platform. The script used to always take
# linux_exe, so on macOS it fetched a Linux ELF the app could not even
# see (locate() looks for Pikafish-MacOS-universal there) — the app
# fell back to the builtin engine even after fetch-engine.sh "worked".
OS="$(uname -s)"
case "$OS" in
    Linux)  EXE="$(toml linux_exe)" ;;
    Darwin) EXE="$(toml macos_exe)" ;;
    *) echo "fetch-engine.sh: unsupported OS: $OS (Linux/macOS only; Windows: fetch-engine.bat)" >&2; exit 1 ;;
esac

[ -n "$VERSION" ] && [ -n "$SOURCE_URL" ] && [ -n "$ARCHIVE_SHA256" ] \
    && [ -n "$NNUE_SHA256" ] && [ -n "$EXE" ] \
    || { echo "fetch-engine.sh: pin file $TOML is incomplete" >&2; exit 1; }

command -v curl >/dev/null || { echo "fetch-engine.sh needs curl" >&2; exit 1; }
# Portable sha256: stock macOS has no sha256sum, it has shasum.
if command -v sha256sum >/dev/null; then
    SHA256="sha256sum"
elif command -v shasum >/dev/null; then
    SHA256="shasum -a 256"
else
    echo "fetch-engine.sh needs sha256sum or shasum" >&2; exit 1
fi

EXTRACT=""
if command -v 7z >/dev/null; then EXTRACT="7z x -y";
elif command -v 7zz >/dev/null; then EXTRACT="7zz x -y";
elif command -v bsdtar >/dev/null; then EXTRACT="bsdtar -xf";
elif python3 -c "import py7zr" 2>/dev/null; then EXTRACT="python3 -m py7zr x";
else
    echo "fetch-engine.sh needs a 7z extractor (7z, 7zz, bsdtar or python3 py7zr)" >&2
    exit 1
fi

TMP="$(mktemp -d "$ROOT/target/.fetch-tmp.XXXXXX" 2>/dev/null || mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

ARCHIVE="$TMP/pikafish.7z"
echo "downloading $SOURCE_URL ..."
curl -fSL --retry 3 -o "$ARCHIVE" "$SOURCE_URL"

echo "verifying archive sha256 ..."
echo "$ARCHIVE_SHA256  $ARCHIVE" | $SHA256 -c - || {
    echo "fetch-engine.sh: archive sha256 MISMATCH — refusing to extract" >&2
    exit 1
}

echo "extracting ..."
mkdir -p "$TMP/extract"
if [ "$EXTRACT" = "python3 -m py7zr x" ]; then
    ( cd "$TMP/extract" && python3 -m py7zr x "$ARCHIVE" >/dev/null )
else
    ( cd "$TMP/extract" && $EXTRACT "$ARCHIVE" >/dev/null )
fi

mkdir -p "$ENGINE_DIR"
for f in "$EXE" pikafish.nnue Copying.txt NNUE-License.md; do
    [ -f "$TMP/extract/$f" ] || { echo "fetch-engine.sh: $f missing from archive" >&2; exit 1; }
    cp "$TMP/extract/$f" "$ENGINE_DIR/"
done
chmod +x "$ENGINE_DIR/$EXE"

echo "verifying NNUE sha256 ..."
echo "$NNUE_SHA256  $ENGINE_DIR/pikafish.nnue" | $SHA256 -c - || {
    echo "fetch-engine.sh: NNUE sha256 MISMATCH" >&2
    exit 1
}

cat > "$ENGINE_DIR/SOURCE.txt" <<EOF
Pikafish $VERSION
Source: https://github.com/official-pikafish/Pikafish
Tag: Pikafish-$VERSION
Commit: $(toml commit)
Release archive: $SOURCE_URL
Archive sha256: $ARCHIVE_SHA256
NNUE sha256: $NNUE_SHA256 (self-computed from the extracted file;
no upstream-published hash exists — see docs/research-pikafish-2026-09-06.md §2)
Build (from source, for reference): git clone the repo above, checkout
the commit, then: cd src && make -j profile-build ARCH=x86-64-universal
(macOS: make -j profile-build ARCH=arm64-universal, then lipo with the
x86-64-universal build; see tools/build-engine.sh)
License: engine is GPLv3 (Copying.txt); pikafish.nnue is NNUE-License,
non-commercial use only (NNUE-License.md).
EOF

echo "done: $ENGINE_DIR/$EXE + pikafish.nnue (Pikafish $VERSION)"
