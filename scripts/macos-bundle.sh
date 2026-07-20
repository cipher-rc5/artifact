#!/usr/bin/env bash
# Build a macOS .app bundle from the universal binary, codesign it with a
# hardened runtime, notarize it, staple the ticket, and package a .dmg.
#
# Required environment (all must be present or the script exits non-zero):
#   APPLE_CERTIFICATE_P12_BASE64  base64 of the Developer ID Application .p12
#   APPLE_CERTIFICATE_PASSWORD    password for that .p12
#   APPLE_CODESIGN_IDENTITY       e.g. "Developer ID Application: Name (TEAMID)"
#   APPLE_ID                      Apple ID used for notarization
#   APPLE_TEAM_ID                 Apple Developer Team ID
#   APPLE_APP_PASSWORD            app-specific password for notarytool
#
# Inputs:
#   $1  version string (e.g. 1.2.3), no leading "v"
#   BIN_SRC (env, optional)  path to the universal binary
#                            (default: target/dist/artifact-macos-universal)
#
# Output: target/dist/ARTIFACT-macos-universal.dmg (stapled + notarized)
set -euo pipefail

VERSION="${1:?usage: macos-bundle.sh <version>}"
BIN_SRC="${BIN_SRC:-target/dist/artifact-macos-universal}"
DIST="target/dist"
APP="${DIST}/ARTIFACT.app"
BUNDLE_ID="com.cipher.artifact"

require() {
  if [ -z "${!1:-}" ]; then
    echo "::error::missing required secret/env: $1" >&2
    exit 1
  fi
}
require APPLE_CERTIFICATE_P12_BASE64
require APPLE_CERTIFICATE_PASSWORD
require APPLE_CODESIGN_IDENTITY
require APPLE_ID
require APPLE_TEAM_ID
require APPLE_APP_PASSWORD

if [ ! -f "$BIN_SRC" ]; then
  echo "::error::universal binary not found at $BIN_SRC" >&2
  exit 1
fi

echo "==> Building .app bundle for ARTIFACT $VERSION"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BIN_SRC" "$APP/Contents/MacOS/artifact"
chmod +x "$APP/Contents/MacOS/artifact"

# App icon is optional; include it if a prebuilt .icns exists.
if [ -f "assets/app-icon.icns" ]; then
  cp "assets/app-icon.icns" "$APP/Contents/Resources/artifact.icns"
  ICON_PLIST="  <key>CFBundleIconFile</key>
  <string>artifact</string>"
else
  ICON_PLIST=""
fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>ARTIFACT</string>
  <key>CFBundleDisplayName</key>
  <string>ARTIFACT</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_ID}</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleExecutable</key>
  <string>artifact</string>
${ICON_PLIST}
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
PLIST

echo "==> Importing signing certificate into a temporary keychain"
KEYCHAIN="$RUNNER_TEMP/artifact-signing.keychain-db"
KEYCHAIN_PASSWORD="$(openssl rand -hex 20)"
CERT_PATH="$RUNNER_TEMP/artifact-cert.p12"

echo "$APPLE_CERTIFICATE_P12_BASE64" | base64 --decode > "$CERT_PATH"
security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security set-keychain-settings -lut 21600 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security import "$CERT_PATH" -P "$APPLE_CERTIFICATE_PASSWORD" \
  -A -t cert -f pkcs12 -k "$KEYCHAIN"
security set-key-partition-list -S apple-tool:,apple:,codesign: \
  -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null
security list-keychains -d user -s "$KEYCHAIN" "$(security list-keychains -d user | tr -d '"')"
rm -f "$CERT_PATH"

cleanup() {
  security delete-keychain "$KEYCHAIN" 2>/dev/null || true
}
trap cleanup EXIT

echo "==> Codesigning with hardened runtime"
codesign --force --deep --timestamp --options runtime \
  --keychain "$KEYCHAIN" \
  --sign "$APPLE_CODESIGN_IDENTITY" \
  "$APP"
codesign --verify --strict --verbose=2 "$APP"

echo "==> Packaging .dmg"
DMG="${DIST}/ARTIFACT-macos-universal.dmg"
rm -f "$DMG"
hdiutil create -volname "ARTIFACT" -srcfolder "$APP" -ov -format UDZO "$DMG"

echo "==> Codesigning the .dmg"
codesign --force --timestamp --options runtime \
  --keychain "$KEYCHAIN" \
  --sign "$APPLE_CODESIGN_IDENTITY" \
  "$DMG"

echo "==> Submitting to Apple notary service (this waits for the result)"
xcrun notarytool submit "$DMG" \
  --apple-id "$APPLE_ID" \
  --team-id "$APPLE_TEAM_ID" \
  --password "$APPLE_APP_PASSWORD" \
  --wait

echo "==> Stapling the notarization ticket"
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"

echo "==> Done: $DMG"
