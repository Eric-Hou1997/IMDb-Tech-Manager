#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
TMP=$(mktemp -d "${TMPDIR:-/tmp}/imdb-tech-manager-test-4.0.4.XXXXXX")
trap 'rm -rf "$TMP"' EXIT HUP INT TERM

# Keep configuration, caches, and browser profiles isolated from the user's
# installed app while exercising the source candidate.
export HOME="$TMP/home"
mkdir -p "$HOME"

for test_file in "$ROOT"/macos/tests/*.py; do
  if [ "$(basename "$test_file")" = "web_ui_dom_contract.py" ]; then
    IMDB_TECH_REQUIRE_BROWSER=1 python3 "$test_file"
  else
    python3 "$test_file"
  fi
done

python3 "$ROOT/macos/engine/mac-engine.py" --self-test
(
  cd "$ROOT/macos"
  GOCACHE="$TMP/go-cache" go vet ./...
  GOCACHE="$TMP/go-cache" go test ./...
  GOCACHE="$TMP/go-cache" go test -race ./...
  GOCACHE="$TMP/go-cache" go build -o "$TMP/IMDbTechManagerCore" .
  "$TMP/IMDbTechManagerCore" --self-check
)
node -e "const fs=require('fs'),s=fs.readFileSync(process.argv[1],'utf8'),m=[...s.matchAll(/<script(?: [^>]*)?>([\\s\\S]*?)<\\/script>/g)];if(m.length!==1)throw Error('expected one consolidated script');new Function(m[0][1]);" "$ROOT/macos/web/index.html"
clang -fobjc-arc -fmodules -fmodules-cache-path="$TMP/clang-cache" -mmacosx-version-min=12.0 -fsyntax-only "$ROOT/macos/native/IMDbTechManagerLauncher.m"
clang -fobjc-arc -fmodules -fmodules-cache-path="$TMP/clang-cache" -mmacosx-version-min=12.0 -fsyntax-only "$ROOT/macos/native/IMDbWebKitFetcher.m"
echo "OK IMDb-Tech-Manager v4.0.4 source, engine, Go core, Web UI DOM, and native WebKit gates"
