#!/usr/bin/env bash
# Build the macOS .app bundle (spec 9.3). Run on macOS from the repo root:
#   ./tools/bundle-macos.sh
# Requires both release binaries already built:
#   cargo build --release --target aarch64-apple-darwin -p xq-app
#   cargo build --release --target x86_64-apple-darwin -p xq-app
# Output: dist/"Cờ Tướng AI.app" (universal binary, ad-hoc signed).
# The DMG is created by .github/workflows/release.yml (hdiutil), not here.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_NAME="Cờ Tướng AI"
DIST="$ROOT/dist"
APP="$DIST/$APP_NAME.app"

command -v lipo >/dev/null || { echo "bundle-macos.sh needs macOS (lipo not found)" >&2; exit 1; }
command -v codesign >/dev/null || { echo "bundle-macos.sh needs macOS (codesign not found)" >&2; exit 1; }

VERSION="$(grep -m1 '^version' "$ROOT/crates/xq-app/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
ARM_BIN="$ROOT/target/aarch64-apple-darwin/release/xiangqi"
X86_BIN="$ROOT/target/x86_64-apple-darwin/release/xiangqi"
for bin in "$ARM_BIN" "$X86_BIN"; do
  [ -f "$bin" ] || { echo "missing binary: $bin (build both targets first)" >&2; exit 1; }
done
[ -f "$ROOT/crates/xq-app/assets/icon.icns" ] || { echo "missing icon.icns (run tools/make-icons.py)" >&2; exit 1; }

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

lipo -create -output "$APP/Contents/MacOS/xiangqi" "$ARM_BIN" "$X86_BIN"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key>
	<string>vn.xiangqi.app</string>
	<key>CFBundleName</key>
	<string>Cờ Tướng AI</string>
	<key>CFBundleDisplayName</key>
	<string>Cờ Tướng AI</string>
	<key>CFBundleExecutable</key>
	<string>xiangqi</string>
	<key>CFBundleIconFile</key>
	<string>icon</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$VERSION</string>
	<key>CFBundleVersion</key>
	<string>$VERSION</string>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
EOF

cp "$ROOT/crates/xq-app/assets/icon.icns" "$APP/Contents/Resources/icon.icns"

# Pikafish engine (spec §8.3): copy engine/ into Resources BEFORE
# codesign, so the engine ships inside the bundle and is signed with
# it. The engine dir is gitignored; fetch it first with
# tools/fetch-engine.sh (or build it with tools/build-engine.sh).
ENGINE_DIR="$ROOT/engine"
if [ -d "$ENGINE_DIR" ]; then
  mkdir -p "$APP/Contents/Resources/engine"
  if [ -f "$ENGINE_DIR/Pikafish-MacOS-universal" ]; then
    cp "$ENGINE_DIR/Pikafish-MacOS-universal" "$APP/Contents/Resources/engine/"
    MACOS_EXE="Pikafish-MacOS-universal"
  elif [ -f "$ENGINE_DIR/pikafish" ]; then
    cp "$ENGINE_DIR/pikafish" "$APP/Contents/Resources/engine/"
    MACOS_EXE="pikafish"
  else
    echo "bundle-macos.sh: no macOS Pikafish binary in $ENGINE_DIR" >&2; exit 1
  fi
  chmod 755 "$APP/Contents/Resources/engine/$MACOS_EXE"
  for f in pikafish.nnue Copying.txt NNUE-License.md SOURCE.txt; do
    [ -f "$ENGINE_DIR/$f" ] || { echo "bundle-macos.sh: missing $ENGINE_DIR/$f" >&2; exit 1; }
    cp "$ENGINE_DIR/$f" "$APP/Contents/Resources/engine/"
  done
  # Sign the engine binary on its own first, then the whole bundle.
  codesign --force --sign - "$APP/Contents/Resources/engine/$MACOS_EXE"
else
  echo "bundle-macos.sh: WARNING: $ENGINE_DIR missing — bundle ships without Pikafish (builtin AI only)" >&2
fi

# Ad-hoc signature (required to run on Apple Silicon). Developer-ID
# signing + notarize are out of scope for 1.0 (spec 9.3.7).
codesign --force --deep --sign - "$APP"

echo "built $APP (version $VERSION)"
lipo -info "$APP/Contents/MacOS/xiangqi"
