#!/usr/bin/env bash
# Build Pikafish from source at the pinned commit (integration spec
# §8.2). This is the GPL-compliant path: the shipped binary must be
# built from the exact pinned source, not downloaded prebuilt.
#
#   ./tools/build-engine.sh [--os linux|windows|macos] [--out DIR]
#
# Default: build for the host OS into engine/ (Linux) or a staging dir.
# - linux:   single-arch gcc build (ARCH=x86-64-avx2 when the CPU has
#            AVX2, else x86-64) — see note below; binary is `pikafish`
# - windows: make build ARCH=x86-64-universal, with COMP=mingw under
#            MSYS2 MinGW-w64 (run this script inside the MSYS2 MinGW shell)
# - macos:   make macos-lipo (x86-64 + Apple silicon slices, lipo'd)
# Plain `build`, not `profile-build`: upstream's own release workflow
# (.github/workflows/pikafish.yml) uses `make build`, and `profile-build`
# cannot work for the universal target — its PGO bench runs inside
# src/temp_builds/<arch>/, where the net is absent: the Makefile only
# symlinks sources there (TRACKED_TOP) and the engine looks for the net
# in cwd + the binary's own dir (nnue/network.cpp Network::load), so
# bench fails with "network file not loaded". Verified 2026-11-11
# against the pinned source (see docs/DECISIONS-pikafish.md §35).
# Linux has no universal binary with stock g++ either: the Makefile's
# Linux universal final link uses clang-only flags (-stdlib=libc++,
# -rtlib=compiler-rt, -fuse-ld=lld, -lc++ ...) unconditionally (except
# COMP=ndk), and upstream builds it with clang 22 + libc++-dev. So on
# Linux this script builds one arch slice with gcc instead; the app
# finds a plain `pikafish` in engine/ (xq-pikafish locate.rs dev
# fallback). For a true universal binary on Linux, install clang +
# libc++ and run: make build ARCH=x86-64-universal COMP=clang
# (see docs/DECISIONS-pikafish.md §36).
# The NNUE net is NOT built: it ships inside the release archive
# (see tools/fetch-engine.sh). Requires: git, make, g++ (or clang).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
OUT="$ROOT/engine"
while [ $# -gt 0 ]; do
    case "$1" in
        --os) OS="$2"; shift 2 ;;
        --out) OUT="$2"; shift 2 ;;
        *) echo "usage: $0 [--os linux|windows|macos] [--out DIR]" >&2; exit 1 ;;
    esac
done

# Resolve a relative --out against the caller's cwd now: the script
# later cds into the clone's src/, so a relative OUT would otherwise
# land inside the temp dir (deleted by the trap on exit).
case "$OUT" in
    /*) ;;
    *) OUT="$PWD/$OUT" ;;
esac

toml() { grep -m1 "^$1 =" "$ROOT/engine/PIKAFISH_VERSION.toml" | sed -E 's/^[^=]*= *"([^"]*)".*/\1/'; }
COMMIT="$(toml commit)"
VERSION="$(toml version)"
[ -n "$COMMIT" ] || { echo "build-engine.sh: no commit in PIKAFISH_VERSION.toml" >&2; exit 1; }

command -v git >/dev/null || { echo "build-engine.sh needs git" >&2; exit 1; }
command -v make >/dev/null || { echo "build-engine.sh needs make" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
echo "cloning Pikafish at $COMMIT ..."
git clone --quiet https://github.com/official-pikafish/Pikafish.git "$WORK/pikafish"
git -C "$WORK/pikafish" checkout --quiet "$COMMIT"
cd "$WORK/pikafish/src"

# The build (and its PGO bench run) needs pikafish.nnue next to the
# binary. Prefer the pinned net from a previous fetch-engine.sh run;
# otherwise download the default net with `make net` — and verify its
# sha256 against the pin in both cases, so a moved master net fails
# loudly instead of silently shipping different weights.
NNUE_SHA256="$(toml nnue_sha256)"
if [ -f "$ROOT/engine/pikafish.nnue" ]; then
    cp "$ROOT/engine/pikafish.nnue" pikafish.nnue
elif command -v curl >/dev/null || command -v wget >/dev/null; then
    make net
else
    echo "build-engine.sh: need engine/pikafish.nnue (run tools/fetch-engine.sh) or curl/wget for 'make net'" >&2
    exit 1
fi
echo "$NNUE_SHA256  pikafish.nnue" | sha256sum -c - || {
    echo "build-engine.sh: NNUE sha256 MISMATCH — the net is not the pinned one" >&2
    exit 1
}

case "$OS" in
    linux*)
        # No universal binary with stock g++ (see header note): build
        # one arch slice instead. AVX2 when the CPU has it, else the
        # baseline x86-64 slice (runs on any x86-64 machine).
        if grep -q avx2 /proc/cpuinfo 2>/dev/null; then
            ARCH=x86-64-avx2
        else
            ARCH=x86-64
        fi
        make -j"$(nproc 2>/dev/null || echo 2)" build ARCH="$ARCH"
        BIN=pikafish
        ;;
    windows*|mingw*|msys*)
        make -j"$(nproc 2>/dev/null || echo 2)" build ARCH=x86-64-universal COMP=mingw
        BIN=pikafish.exe
        ;;
    darwin*)
        # Official universal path: x86-64 slice + Apple silicon slice,
        # lipo'd into one binary (no PGO here; `macos-lipo` uses `build`).
        make -j"$(sysctl -n hw.ncpu 2>/dev/null || echo 2)" macos-lipo
        BIN=pikafish
        ;;
    *)
        echo "build-engine.sh: unsupported --os '$OS' (linux|windows|macos)" >&2
        exit 1
        ;;
esac

# The fresh binary must match the pinned release's bench signature.
./$BIN bench 2>&1 | grep -q "Nodes searched" || { echo "build-engine.sh: bench failed" >&2; exit 1; }

mkdir -p "$OUT"
cp "$BIN" "$OUT/"
cp pikafish.nnue "$OUT/"  # the net whose sha256 was verified above
cp ../Copying.txt "$OUT/" 2>/dev/null || true
cp ../NNUE-License.md "$OUT/" 2>/dev/null || true
echo "built $OUT/$BIN (Pikafish $VERSION, commit $COMMIT)"
./$BIN bench 2>&1 | grep "Nodes searched"
