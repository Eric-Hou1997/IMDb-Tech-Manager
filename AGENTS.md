# IMDb-Tech-Manager repository instructions

These instructions are part of the public repository. Apply them to every change in this repository, including changes made in a fork. A fork does not imply authority to publish an official upstream release.

## Maintainer shorthand

- In maintainer conversations only, `tech软件` means the IMDb-Tech-Manager product in this repository, and `card软件` means the separate Tech Card Manager product.
- These shorthand terms are coordination language, not product branding. Never place `tech软件` or `card软件` in user-facing UI, logs, packages, release names, or marketing copy.

## Repository identity and source of truth

- This is the `IMDb-Tech-Manager` product repository. Product source starts at `v4.0.0`; do not import earlier product source, release artifacts, changelogs, tags, or version labels. Documentation-only GitHub history may be retained when synchronizing the public repository, but it must not be presented as earlier product source history.
- The authoritative development line is `main`. Follow the maintainer's branch and review workflow, and never rewrite published history without explicit authorization.
- The repository is organized by product, not permanently by operating system. The currently supported implementation is the Apple Silicon macOS application under `macos/`. A future Windows port belongs here only when explicitly scoped, and it must preserve this product's responsibilities and identity.
- Do not copy the read-only Tech Card Manager product into this repository or rename this product to `Tech Card Manager`. A platform port of this product is not the same thing as the separate Card Manager product.
- Platform-specific files or build stubs are not proof that a platform is supported. Support requires a complete product flow, packaging rules, tests, and real-platform acceptance.
- Inspect the current source, root README, release-note sources, tests, and embedded assets before editing. Extend the existing Go Core, local Web UI, Python engine, and native launcher architecture; do not replace the desktop stack or rewrite the product from scratch.
- Tests are evidence, not a complete behavior specification. Add characterization coverage before changing NFO ownership, XML mutation, AI retry/accounting, task scope, persistence, or platform lifecycle behavior.

## Repository and contribution hygiene

- 当用户找到一个 bug 或提出了一个很棒的想法时，请先回应：牛逼
- Keep product code, documentation, filenames, UI text, package metadata, and artifacts free of prerelease branding and pre-repository product version labels. Third-party names and compatibility data, such as a browser channel name or a fixture's external-tool version, are not product branding and must remain accurate.
- Do not commit local configuration, credentials, API keys, tokens, caches, browser profiles, logs, generated binaries, packaged applications, or release archives.
- The repository is licensed under Apache License 2.0. Retain `LICENSE` and `NOTICE` in redistributions, keep author attribution as `侯雁泽`, and do not alter the license, trademark policy, or asset-redistribution claims without an explicit maintainer decision.
- Treat `packaging/` as release-input source and `tools/build-release.sh` as a release recipe. Their presence does not mean that a package has been built, validated, signed for distribution, notarized, tagged, or published.
- Do not claim that a source checkout, cross-build, fixture, mock, or static contract proves real desktop behavior.
- Preserve unrelated contributor changes. Do not perform broad cleanup, global replacement, history rewriting, dependency upgrades, or architecture migrations unless they are in the reviewed task scope.
- Do not assume access to a maintainer's local folders, private issue archives, previous repositories, credentials, or release systems. The public repository must remain understandable and testable on its own.

## Product responsibilities

- macOS is currently the Technical Specs producer, NFO inspector/editor, Local/AI Tag producer, ownership manager, and batch task center.
- Movie and TV are separate spaces with independent search, filters, selection, scopes, and task history.
- All tag-generation entry points must present `AI 生成标签` and `规则生成标签` together. Full-library processing is never the default scope.
- The Inspector must show all root tags and distinguish External, Generated, and Manual Tech Tags from authoritative ownership metadata, never from text resemblance.
- Editing an External Tag preserves external ownership. Editing a Generated Tag converts it to protected Manual ownership.
- Editing effective Technical Specs marks derived Tags stale; it must not trigger an implicit IMDb refresh or tag rebuild.
- User-visible errors identify the affected title, year, IMDb ID, full NFO path, media kind, and task when those fields are available, and UI errors must link back to the affected NFO.

## NFO safety and ownership

- Spec Agent may write only `<technicalspecs>` and must never add or remove root `<tag>` values. Technical Specs are the fact layer; Tags are derived.
- A tag generator may mutate only Generated Tech Tags that this product actually inserted or already authoritatively owns. Automated or background work must never delete, claim, or rewrite Manual Tech Tags, TMM tags, ordinary user tags, External Tags, or another application's tags.
- The only permitted external-tag deletion is an explicit, confirmed, user-initiated Inspector action with a verified backup and undo path. Automated pipelines must still return `unsafe-skip`.
- Build and validate the complete replacement candidate before removing an old owned Generated Tag. Unknown, missing, or conflicting ownership must fail closed with `unsafe-skip` and leave the NFO unchanged.
- Preserve XML validity, UTF-8 BOM and newline style, file mode, backups, source-hash compare-and-swap, temporary-file fsync, and atomic replacement.
- Resolve and validate every requested path against configured real library roots. Symlink or path ambiguity must fail closed.
- Write the embedded ownership manifest atomically with the NFO first. Mirror a sidecar only after the NFO write succeeds.

## AI behavior and accounting

- Read `choices[0].finish_reason` before parsing content. Treat `finish_reason=length` as `output-truncated`; retry only with a larger output limit, never with an identical request.
- Keep truncated output, malformed JSON, schema-invalid output, provider-response failures, HTTP transient errors, authentication, quota, and rate-limit failures as distinct structured states.
- Count every real HTTP attempt and every successful HTTP usage response before validating the result. Cache hits cost zero in the current run but retain separately displayed historical usage.
- Skip an unchanged repeated AI failure during ordinary runs. Explicit retry-failed actions or changed input/configuration may retry.
- Keep simple structured Qwen tasks non-thinking by default and retain prompt caching and compact per-NFO payload behavior.

## Product maturity and lifecycle

- A feature is complete only when its real chain is proven: authoritative input -> backend state -> persisted artifact -> served/runtime behavior -> visible UI result -> stop and cleanup behavior.
- Status must distinguish requested, running, disk-ready, service-served, client-loaded, rendered, stopped, failed, and unverified states. Never report success from an intermediate prerequisite.
- Give every window, tray/menu item, browser profile, child process, timer, observer, job, port, lease, temporary file, and service one explicit owner with idempotent creation and synchronously verified shutdown.
- Core setup, permissions, progress, cancellation, recovery, and actionable failures must be visible in normal product flows. Long work must provide honest phase/progress feedback.
- Preserve structured failure reasons and surface concise actionable messages; do not swallow errors that affect visible behavior.
- Proactively audit normal, first-run, empty, slow, cancelled, minimized/restored, repeated-click, second-launch, upgrade, rollback, partial-failure, crash-recovery, offline, permission-denied, and exit paths.
- Add behavioral and state-transition regression coverage for every defect and adjacent negative path. Source-string assertions alone are not sufficient proof.

## Verification and release boundary

- Keep Python 3.8-3.11 compatibility unless the product requirement explicitly changes.
- Preserve the Casino Royale 1967 and Pacific Rim: Uprising real regression fixtures and their byte-level properties where covered by tests.
- For relevant changes, run Python contracts, engine self-test, Go vet/test/race/build, JavaScript syntax and DOM checks, architecture checks, and native Objective-C syntax checks. Report commands, results, migrations, modified files, and all untested real-platform items.
- Real macOS permissions, WebKit/window lifecycle, browser behavior, Gatekeeper, signing, and notarization are explicit acceptance boundaries; never infer them from source validation.
- The current formal macOS deliverable is an Apple Silicon arm64 `.app` contained in a ZIP. Never place a bare `.app` or executable in `releases/`. Do not infer packaging rules for a future platform port; define and verify them when that port is explicitly undertaken.
- Audit the exact release range and all primary flows before packaging. Create a new version rather than overwrite an artifact.
- Do not build a release package, create a tag, push, or publish a release unless the maintainer explicitly requests that release after review. A successful build alone is not release approval.
