# Stix release checklist

CI is reused by both build workflows: a failed quality/security job blocks beta
publication and stable draft builds. Beta tags point to the exact tested SHA.
Stable tag pushes build a draft only. Publish manually or with an authorized
user token (events created by `GITHUB_TOKEN` do not start another workflow).

Use this checklist for stable releases. Beta builds may skip the updater-feed
steps, but they must pass the same code, privacy, storage, and accessibility gates.

## Repository secrets

Set these in GitHub → Settings → Secrets and variables → Actions. `GITHUB_TOKEN`
is provided by Actions. Do not add `HOMEBREW_TAP_TOKEN` or `VERCEL_DEPLOY_HOOK`.

- `TAURI_SIGNING_PRIVATE_KEY` — contents of `.updater/stix.key`. Never commit that file.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — password for that key, when the key has one.
- `APPLE_CERTIFICATE` — Developer ID Application `.p12`, base64-encoded.
- `APPLE_CERTIFICATE_PASSWORD` — password for the `.p12`.
- `APPLE_SIGNING_IDENTITY` — certificate name from `security find-identity`.
- `APPLE_TEAM_ID` — Apple Developer Team ID.
- `APPLE_ID` — Apple ID used for notarization.
- `APPLE_PASSWORD` — app-specific password for that Apple ID.

The updater feed is `https://github.com/niko-zvt/stix/releases/latest/download/latest.json`.
The matching public key is already in `src-tauri/tauri.conf.json`. Until a release
publishes `latest.json`, update checks fail closed.

## 1. Prepare the release

- [ ] Start from an up-to-date `main` with a clean working tree.
- [ ] Confirm every intended change has reached `main`; do not release directly from a feature branch.
- [ ] Confirm `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `CHANGELOG.md`, and the tag use the same version.
- [ ] Review open release-blocking issues, security advisories, and dependency update pull requests.
- [ ] Confirm the release notes describe migrations, compatibility changes, and recovery steps.

## 2. Run automated gates

```bash
bun install --frozen-lockfile
bun audit --audit-level=high
bun run check:platform
bun run build
bun run check:bundle
bun run test

swift test --package-path src-tauri/darwinkit

cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-features
cargo audit --file src-tauri/Cargo.lock
cargo deny --manifest-path src-tauri/Cargo.toml check advisories
```

- [ ] All commands pass without ignored or advisory-only failures.
- [ ] The capture entry remains below the enforced bundle budget.
- [ ] The full-text search performance smoke test remains within its budget.

## 3. Exercise user data and recovery

Use a scratch notes directory. Stable and beta builds can share settings, so never point a test build at irreplaceable data.

- [ ] Create, edit, rename, move, and delete notes in local, nested, and custom folders.
- [ ] With capture, viewing, pinned, and full-editor windows open, type and immediately use Cmd+Q/application-menu Quit, tray Quit, Dock Quit, and log out of a disposable macOS account. Confirm every final draft survives each supported orderly shutdown. A failed save or missing acknowledgement must cancel Quit and restore editing; retry must work without duplicate notes. Exercise AppKit termination separately from Tauri's exit callback.
- [ ] Restore a note from Trash and confirm a name conflict never overwrites an existing note.
- [ ] Paste and move image assets; confirm crafted `../`, absolute, and symlinked paths cannot escape the configured vault.
- [ ] Edit a note from another app and confirm Stix refreshes without displaying a partial write.
- [ ] Run Vault Health against healthy, unwritable, and deliberately out-of-sync scratch vaults; export diagnostics.
- [ ] Lock and unlock a note, relaunch, test idle/sleep relocking, and verify recovery-key export.
- [ ] Confirm legacy lock-key migration succeeds before the old file is removed.

## 4. Check privacy and accessibility

- [ ] Confirm the app makes no analytics or telemetry request on launch.
- [ ] Confirm remote Markdown images are blocked by default and require the explicit setting to load.
- [ ] Complete capture, search, move, delete/restore, settings, and lock/unlock using only the keyboard.
- [ ] Run the same primary workflows with VoiceOver, including dialog focus entry/return and live status announcements.
- [ ] Check light, dark, reduced-motion, English, and Simplified Chinese modes.

## 5. Build and inspect artifacts

- [ ] Build and launch a local app with `./scripts/build-dev.sh build` on the available architecture.
- [ ] Build Apple Silicon and Intel release artifacts in CI.
- [ ] Confirm each `.app` declares macOS 14, contains the matching DarwinKit sidecar, and launches on a clean macOS 14+ account.
- [ ] Verify global shortcuts, Accessibility permission flow, microphone permission, dictation, Finder “Open With,” tray behavior, and window restoration.
- [ ] Verify signatures with `codesign`, notarization with `spctl`, and both DMGs before publishing.
- [ ] Verify both updater archives against the configured updater public key. Confirm `latest.json` contains the release version and both Darwin architecture entries, with working asset URLs and signatures matching their archives; the JSON metadata is not independently signed.

## 6. Publish and monitor

- [ ] Publish the GitHub draft only after both architectures and DMGs are present.
- [ ] Install from the DMG and upgrade from the previous stable release through the in-app updater.
- [ ] Confirm stable users cannot receive beta artifacts and beta builds cannot update the stable feed.
- [ ] Watch crash/support/security channels after release and keep the previous signed build available for rollback.
- [ ] Close only issues verified in the published build, then update the roadmap and release links.
