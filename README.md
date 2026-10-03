<p align="center">
  <img src="app-icon.svg" width="128" height="128" alt="Stix icon">
</p>

<h1 align="center">Stix</h1>

<p align="center">
  <strong>Instant thought capture for macOS.</strong><br>
  Press a shortcut. Type your thought. Get back to work.<br>
  Under 3 seconds. Every time.
</p>

<p align="center">
  A fork of <a href="https://github.com/0xMassi/stik_app">Stik</a>.
</p>

<p align="center">
  <a href="https://github.com/niko-zvt/stix/releases/latest">Download</a> &middot;
  <a href="ROADMAP.md">Roadmap</a> &middot;
  <a href="CHANGELOG.md">Changelog</a>
</p>

<p align="center">
  <a href="https://github.com/niko-zvt/stix/releases/latest"><img src="https://shieldcn.dev/github/niko-zvt/stix/release.svg?color=7C3AED" alt="Latest release"></a>
  <img src="https://shieldcn.dev/github/niko-zvt/stix/license.svg" alt="License">
  <img src="https://shieldcn.dev/badge/platform-macOS-000.svg?logo=apple" alt="macOS">
</p>

<p align="center">
  <img src=".github/assets/hero.gif" width="600" alt="Stix demo">
</p>

<p align="center">
  <a href="https://github.com/niko-zvt/stix/releases/latest"><img src="https://shieldcn.dev/badge/Download_Stix_for_Mac-7C3AED.svg?logo=apple&logoColor=white&size=lg" alt="Download Stix for Mac"></a>
</p>

---

## Why Stix?

Every note app wants to be your second brain. Stix just wants to catch your thought before it disappears.

No onboarding. No account. No sync setup. Hit `Ctrl+Option+S`, type, close. Your note is saved as a plain markdown file. That's it.

## Install

### Download for Mac

Grab the latest `.dmg` from the [Releases page](https://github.com/niko-zvt/stix/releases/latest).

> Requires **macOS 14+ (Sonoma)**. On first launch, grant Accessibility permissions when prompted (needed for global shortcuts).

### Update

Stix includes a built-in auto-updater. It downloads a signed release in the background, and the update applies the next time the app starts. Until a release publishes `latest.json`, the check finds nothing and stays quiet.

### Beta builds

Every push to `develop` produces a signed build, published under [Releases](https://github.com/niko-zvt/stix/releases) as a prerelease. It installs as **Stix Beta** beside your stable copy, and Settings shows a BETA pill next to the title so you always know which one you're in.

A beta reads the same notes and settings as the stable app. Before you test anything destructive, send it somewhere harmless: Settings, Folders, Notes directory.

Stable users never see these builds. Beta releases ship no updater artifacts and leave the update feed alone.

## Features

**Capture.** A global shortcut summons a floating post-it over whatever you're doing. Type, close, done. Every note lands in `~/Documents/Stix/` as markdown.

**Voice.** `Ctrl+Option+D` transcribes speech straight into the note you're editing. `Ctrl+Option+V` opens a fresh post-it already listening. WhisperKit runs the model on the Neural Engine, so the audio never leaves your Mac.

**Clip.** `Ctrl+Option+C` takes whatever text you've selected in Safari, Terminal, VS Code, or any standard text field and appends it to a Clips note. No copy, no paste, no window switching.

**Organize.** Folders you name. Move a note with one keystroke. Search everything from the command palette.

**Pin.** Park a note on your desktop as a floating sticky.

**Rich editor.** Source-mode markdown with syntax highlighting, `==highlights==`, `[[wiki-links]]`, collapsible headings, image paste and drop, task lists, and editable tables. Type `/` for slash commands. Write your own templates in Settings. Vim mode if you want it.

**Lock.** Encrypt a note with AES-256 and open it with Touch ID or your device password. Set an idle timeout, or relock everything when the Mac sleeps.

**On-device AI.** Semantic search, folder suggestions, and note embeddings through Apple's NaturalLanguage framework. No cloud, no API keys, nothing sent anywhere.

**Language.** English and Simplified Chinese (简体中文). Press `Ctrl+Option+,` and pick one from the first card in the Appearance tab. Every open window switches without a restart. Leave it on "Follow system language" and Stix reads your macOS setting.

**Sync.** Point Stix at any folder Dropbox or Syncthing already watches, or push a folder to a git remote.

**Share.** Copy a note as rich text, markdown, or an image. Push a folder to a git remote and Stix keeps it synced in the background.

**Import.** Pull notes out of Apple Notes from inside Stix. Nothing to export first.

**Themes.** System, Light, Dark, or your own colors. Follows macOS appearance as it changes.

**Remember.** A capture streak counts your daily habit. "On This Day" brings back notes from past years.

## Keyboard Shortcuts

All shortcuts are customizable in Settings.

| Shortcut | Action |
|----------|--------|
| `Ctrl+Option+S` | Capture a new note in Inbox |
| `Ctrl+Option+1` / `2` / `3` | Capture into Work, Ideas, or Personal |
| `Ctrl+Option+D` | Start or stop dictation in the current note |
| `Ctrl+Option+V` | New post-it, dictating from the start |
| `Ctrl+Option+C` | Append the selected text from any app to Clips |
| `Ctrl+Option+P` | Command palette (search + folders) |
| `Ctrl+Option+M` | Command palette (second shortcut) |
| `Ctrl+Option+L` | Reopen last note |
| `Ctrl+Option+E` | Open the full editor |
| `Ctrl+Option+.` | Zen mode (inside a Stix window) |
| `Ctrl+Option+,` | Open settings |

## Your Data, Your Machine

- Notes are **plain markdown files** in `~/Documents/Stix/` -- open them in any editor
- All AI runs **on-device** via Apple frameworks -- nothing is sent anywhere
- No account, no cloud service, and no analytics. Note content never leaves your Mac unless you turn on git sharing yourself.
- Settings stored locally in `~/.stix/`
- Want sync? Point the notes folder at a directory Dropbox, Syncthing, or another folder sync already watches.

## Build from Source

### Prerequisites

- macOS 14+ (Sonoma)
- [Xcode Command Line Tools](https://developer.apple.com/xcode/resources/) (`xcode-select --install`)
- [Rust](https://rustup.rs/) stable
- [Node.js](https://nodejs.org/) 20+
- [Bun](https://bun.com/docs/installation) 1.4.1 (pinned in `.bun-version`)

### Build

```bash
git clone --recurse-submodules https://github.com/niko-zvt/stix.git
cd stix
bun install --frozen-lockfile
./scripts/build-dev.sh dev    # Development with hot reload
./scripts/build-dev.sh build  # Local .app bundle for testing
```

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Frontend | React 19, TypeScript, Tailwind CSS, CodeMirror 6 |
| Backend | Rust, Tauri 2.0 |
| AI | DarwinKit (Swift CLI wrapping Apple NaturalLanguage framework) |
| Storage | Local filesystem (`.md` files), optional git sync |

## Contributing

Contributions are welcome. Bun 1.4.1 is the canonical JavaScript package manager; keep `bun.lock` current and do not add a second lockfile. Node and Vitest remain the build-tool runtime and test runner. Please open an issue first to discuss what you'd like to change.

```bash
# Check Rust code
cd src-tauri && cargo check

# Format Rust code
cd src-tauri && cargo fmt

# Type check frontend
bun run build

# Run tests
bun run test
cd src-tauri && cargo test
```

Maintainers preparing a stable build should follow the [release checklist](docs/release-checklist.md).

**Translations.** Every string lives in [`src/i18n/locales/`](src/i18n/locales/). Copy `en.ts`, translate the values, and add your locale to `LOCALES` in `src/i18n/index.ts`. The catalogue is typed against English, so a missing key breaks the build instead of shipping a blank label, and `bun run test` checks both catalogues for drift. Corrections to [`zh-CN.ts`](src/i18n/locales/zh-CN.ts) are welcome: read it top to bottom without opening a single component.

## Support

Questions, bugs, and feature requests go to [GitHub issues](https://github.com/niko-zvt/stix/issues).

## License

[MIT](LICENSE)
