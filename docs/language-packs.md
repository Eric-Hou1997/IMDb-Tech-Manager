# Language packs

IMDb Tech Manager keeps `zh-CN`, `zh-Hant`, and `en-US` in the application. Other registered languages are release assets and are installed under `~/Library/Application Support/IMDb Tech Manager/Language Packs/`.

`language-packs/catalog.json` is the release source of truth. The byte-identical copy at `macos/language_catalog.json` is embedded in the application. A catalog maps every locale to one exact revision, original GitHub Release tag, asset name, SHA-256 digest, and message-set hash. A later app release may retain an older `released_with` value when that locale did not change; the client never searches backward through Releases.

The embedded catalog is active only when its `app_version` exactly equals the running app version. This lets the next release catalog be reviewed while source version labels still identify the last completed release, without allowing an older binary to download future packs. The final version closeout activates the matching catalog.

At startup the app restores the configured external language and every other external locale that has an older installed revision. Locales the user never downloaded are not fetched automatically. Consequently an app upgrade updates all previously installed packs to the exact revisions in its catalog without exposing a separate language-pack update action.

Human-reviewable translations live at `language-packs/<locale>/r<revision>/translations.json`. Message keys are stable English presentation strings. `tools/build-language-packs.py` converts them to stable IDs and produces deterministic ZIP bytes. If any translation changes, increment that locale revision and publish the new asset with the app release. If it does not change, copy the prior descriptor unchanged into the new app catalog.

Run `python3 tools/build-language-packs.py --update-catalog` after an intentional source or revision change, review both catalog copies, then run the script without that flag as a clean verification. Official packaging passes the exact app version and emits only descriptors whose `released_with` equals that version.

Normal source checks permit translators to fill a revision incrementally. Formal packaging additionally passes `--require-complete`, which extracts the registered Web UI, Core/Engine, and native message inventory from source and refuses a release while any downloadable locale is missing a key. An English fallback is therefore useful during development but is not accepted as evidence that a v4.1.0 language pack is release-complete.

Language packs contain presentation text only. They cannot alter NFO data, Technical Specs keys, ownership, Tag values, cache schemas, JSON protocols, prompts, or model behavior. A task records its language, pack revision, and embedded catalog hash when it starts. Installed old revisions remain available for history and rollback, and old log bytes are never rewritten.

After the Core verifies an external pack, the native launcher keeps only its allow-listed native menu and alert strings in macOS user defaults. That presentation cache lets an early startup failure remain understandable before WebKit is available; it cannot select a language or affect Core data, and a normal launch refreshes it from the exact catalog-bound pack.

The ZIP digest authenticates the downloaded archive. Its authenticated manifest also records a SHA-256 digest for every extracted section; the app rechecks those files before treating a pack as installed. A missing, modified, wrong-product, wrong-release, wrong-revision, or wrong-schema pack is rejected and can be atomically restored from the exact release asset.
