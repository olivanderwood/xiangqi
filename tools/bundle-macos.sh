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

# Ad-hoc signature (required to run on Apple Silicon). Developer-ID
# signing + notarize are out of scope for 1.0 (spec 9.3.7).
codesign --force --deep --sign - "$APP"

echo "built $APP (version $VERSION)"
lipo -info "$APP/Contents/MacOS/xiangqi"
