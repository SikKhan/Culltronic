#!/usr/bin/env bash
# Build a universal macOS .app bundle and ZIP release artifact.
set -euo pipefail

cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)
if [ -z "$version" ]; then
    echo "error: cannot read workspace version from Cargo.toml" >&2
    exit 1
fi

targets=(aarch64-apple-darwin x86_64-apple-darwin)
rustup target add "${targets[@]}"
for target in "${targets[@]}"; do
    cargo build --locked --release --target "$target" -p culltronic-ui
done

mkdir -p dist
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
app="$tmp/Culltronic.app"
contents="$app/Contents"
mkdir -p "$contents/MacOS" "$contents/Resources"
lipo -create \
    target/aarch64-apple-darwin/release/culltronic-ui \
    target/x86_64-apple-darwin/release/culltronic-ui \
    -output "$contents/MacOS/culltronic-ui"
cp crates/culltronic-ui/assets/branding/culltronic.icns "$contents/Resources/"
cp LICENSE "$contents/Resources/"

cat > "$contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Culltronic</string>
  <key>CFBundleDisplayName</key><string>Culltronic</string>
  <key>CFBundleIdentifier</key><string>io.github.sikkhan.culltronic</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleExecutable</key><string>culltronic-ui</string>
  <key>CFBundleIconFile</key><string>culltronic.icns</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF

codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"
archive="dist/culltronic-${version}-macos-universal.zip"
ditto -c -k --sequesterRsrc --keepParent "$app" "$archive"
shasum -a 256 "$archive" > "$archive.sha256"
echo "==> $archive"
