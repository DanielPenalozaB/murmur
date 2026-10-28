#!/bin/sh
# Packages the built Murmur.app into a .dmg (drag to Applications) and a .zip.
#   pnpm tauri build --bundles app && ./scripts/package-release.sh
# Uses hdiutil rather than Tauri's DMG step, which scripts Finder and fails headless.
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n 's/^ *"version": "\(.*\)",$/\1/p' src-tauri/tauri.conf.json | head -1)
target=${CARGO_TARGET_DIR:-target}
app="$target/release/bundle/macos/Murmur.app"
out="$target/release/dist"
[ -d "$app" ] || { echo "no $app; run: pnpm tauri build --bundles app" >&2; exit 1; }

rm -rf "$out" && mkdir -p "$out/stage"
cp -R "$app" "$out/stage/"
ln -s /Applications "$out/stage/Applications"
hdiutil create -volname Murmur -srcfolder "$out/stage" -ov -format UDZO \
  "$out/Murmur_${version}_aarch64.dmg" >/dev/null
(cd "$out/stage" && ditto -c -k --keepParent Murmur.app "../Murmur_${version}_aarch64.app.zip")
rm -rf "$out/stage"
(cd "$out" && shasum -a 256 Murmur_* > SHA256SUMS.txt)
ls -lh "$out"
