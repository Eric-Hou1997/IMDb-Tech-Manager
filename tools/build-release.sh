#!/bin/sh
set -eu

# Release-only recipe. The archive is versioned; the app bundle name remains
# stable so Finder can replace the prior installation without manual renaming.
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
OUT="$ROOT/releases"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/imdb-tech-manager-4.0.0.XXXXXX")
trap 'rm -rf "$TMP"' EXIT HUP INT TERM
GO_CACHE="$TMP/go-cache"
CLANG_CACHE="$TMP/clang-cache"
MAC_APP_NAME="IMDb Tech Manager.app"
MAC_ZIP_NAME="IMDb-Tech-Manager-macOS-v4.0.0-AppleSilicon-App.zip"
SHA_NAME="SHA256SUMS-IMDb-Tech-Manager-macOS-v4.0.0.txt"
README_NAME="README-macOS-v4.0.0.txt"
CHANGELOG_NAME="CHANGELOG-macOS-v4.0.0.txt"

for target in "$OUT/$MAC_ZIP_NAME" "$OUT/$SHA_NAME" "$OUT/$README_NAME" "$OUT/$CHANGELOG_NAME"; do
  if [ -e "$target" ]; then
    echo "refusing to overwrite existing release target: $target" >&2
    exit 2
  fi
done

sh "$ROOT/tools/test-source.sh"

MAC_APP="$TMP/$MAC_APP_NAME"
mkdir -p "$OUT" "$MAC_APP/Contents/MacOS" "$MAC_APP/Contents/Resources"
(
  cd "$ROOT/macos"
  GOCACHE="$GO_CACHE" GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go test ./...
  GOCACHE="$GO_CACHE" GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go build -trimpath -ldflags="-s -w" -o "$MAC_APP/Contents/MacOS/IMDbTechManagerCore" .
)
clang -arch arm64 -fobjc-arc -fmodules -fmodules-cache-path="$CLANG_CACHE" -mmacosx-version-min=12.0 "$ROOT/macos/native/IMDbTechManagerLauncher.m" -framework Cocoa -framework WebKit -o "$MAC_APP/Contents/MacOS/IMDbTechManagerLauncher"
clang -arch arm64 -fobjc-arc -fmodules -fmodules-cache-path="$CLANG_CACHE" -mmacosx-version-min=12.0 "$ROOT/macos/native/IMDbWebKitFetcher.m" -framework Cocoa -framework WebKit -o "$MAC_APP/Contents/MacOS/IMDbWebKitFetcher"
cp "$ROOT/packaging/Info.plist" "$MAC_APP/Contents/Info.plist"
cp "$ROOT/macos/assets/AppIcon.icns" "$MAC_APP/Contents/Resources/AppIcon.icns"
chmod 755 "$MAC_APP/Contents/MacOS/IMDbTechManagerLauncher" "$MAC_APP/Contents/MacOS/IMDbTechManagerCore" "$MAC_APP/Contents/MacOS/IMDbWebKitFetcher"

"$MAC_APP/Contents/MacOS/IMDbTechManagerCore" --self-check
codesign --force --deep --sign - "$MAC_APP"
codesign --verify --deep --strict "$MAC_APP"
ditto -c -k --sequesterRsrc --keepParent "$MAC_APP" "$OUT/$MAC_ZIP_NAME"
unzip -t "$OUT/$MAC_ZIP_NAME" >/dev/null
(
  cd "$OUT"
  shasum -a 256 "$MAC_ZIP_NAME" > "$SHA_NAME"
  shasum -a 256 -c "$SHA_NAME"
)
cp "$ROOT/packaging/README.txt" "$OUT/$README_NAME"
cp "$ROOT/packaging/CHANGELOG.txt" "$OUT/$CHANGELOG_NAME"

if find "$OUT" -maxdepth 1 -type d -name '*.app' -print -quit | grep -q .; then
  echo "error: bare .app found in releases/" >&2
  exit 3
fi
if find "$OUT" -maxdepth 1 -type f \( -name '*.exe' -o -name '*Core' \) -print -quit | grep -q .; then
  echo "error: bare executable found in releases/" >&2
  exit 3
fi

echo "4.0.0 macOS release generated (ZIP only): $OUT/$MAC_ZIP_NAME"
