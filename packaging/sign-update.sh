#!/bin/sh
# Sign a release ZIP for the in-app updater. The private key is deliberately
# outside this repository. Set IMDB_TECH_UPDATE_PRIVATE_KEY to its PEM path.
set -eu
archive=${1:?usage: sign-update.sh /path/to/IMDb-Tech-Manager.zip}
key=${IMDB_TECH_UPDATE_PRIVATE_KEY:?set IMDB_TECH_UPDATE_PRIVATE_KEY to the Ed25519 PEM file}
openssl pkeyutl -sign -rawin -inkey "$key" -in "$archive" | base64 > "$archive.sig"
printf '%s\n' "wrote $archive.sig"
