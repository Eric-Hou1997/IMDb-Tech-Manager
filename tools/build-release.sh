#!/bin/sh
set -eu

# Release-only recipe. The archive is versioned; the app bundle name remains
# stable so Finder can replace the prior installation without manual renaming.
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
OUT="$ROOT/releases"
VERSION="4.0.4"
ARTIFACT_BASE="ITM-v${VERSION}-MacOS-AArch64-APP"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/imdb-tech-manager-${VERSION}.XXXXXX")
trap 'rm -rf "$TMP"' EXIT HUP INT TERM
GO_CACHE="$TMP/go-cache"
CLANG_CACHE="$TMP/clang-cache"
MAC_APP_NAME="IMDb Tech Manager.app"
MAC_ZIP_NAME="${ARTIFACT_BASE}.zip"
SHA_NAME="${ARTIFACT_BASE}-SHA256SUMS.txt"
SIG_NAME="$MAC_ZIP_NAME.sig"
README_NAME="${ARTIFACT_BASE}-README.txt"
CHANGELOG_NAME="${ARTIFACT_BASE}-CHANGELOG.txt"

for target in "$OUT/$MAC_ZIP_NAME" "$OUT/$SIG_NAME" "$OUT/$SHA_NAME" "$OUT/$README_NAME" "$OUT/$CHANGELOG_NAME"; do
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
cp -R "$ROOT/packaging/zh-Hans.lproj" "$ROOT/packaging/en.lproj" "$MAC_APP/Contents/Resources/"
cp "$ROOT/macos/assets/AppIcon.icns" "$MAC_APP/Contents/Resources/AppIcon.icns"
chmod 755 "$MAC_APP/Contents/MacOS/IMDbTechManagerLauncher" "$MAC_APP/Contents/MacOS/IMDbTechManagerCore" "$MAC_APP/Contents/MacOS/IMDbWebKitFetcher"

"$MAC_APP/Contents/MacOS/IMDbTechManagerCore" --self-check
codesign --force --deep --sign - "$MAC_APP"
codesign --verify --deep --strict "$MAC_APP"
ditto -c -k --sequesterRsrc --keepParent "$MAC_APP" "$OUT/$MAC_ZIP_NAME"
unzip -t "$OUT/$MAC_ZIP_NAME" >/dev/null
if [ -z "${IMDB_TECH_UPDATE_PRIVATE_KEY:-}" ]; then
  echo "error: set IMDB_TECH_UPDATE_PRIVATE_KEY to the Ed25519 private key before packaging a release" >&2
  exit 4
fi
IMDB_TECH_UPDATE_PRIVATE_KEY="$IMDB_TECH_UPDATE_PRIVATE_KEY" sh "$ROOT/packaging/sign-update.sh" "$OUT/$MAC_ZIP_NAME"
(
  cd "$OUT"
  shasum -a 256 "$MAC_ZIP_NAME" "$SIG_NAME" > "$SHA_NAME"
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

echo "${VERSION} macOS release generated (ZIP only): $OUT/$MAC_ZIP_NAME"
