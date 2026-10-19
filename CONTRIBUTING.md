# Contributing to Kara Download Manager

Thanks for helping! Bug reports, ideas, translations, docs and code are all welcome. By taking part you agree
to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to help

- **Report a bug** — open an [issue](https://github.com/kamisama-v0/kara-dm/issues/new/choose) with your OS,
  kara-dm version (`kara-dm --version`), browser + extension version, and the output of `kara-dm --selftest`.
- **Suggest a feature** — open an issue first so we can agree on the approach before you write code.
- **Translate** — the UI is currently Indonesian; English and other languages are very welcome (see
  [Translations](#translations)).
- **Send a pull request** — small, focused PRs are easiest to review.

Security problems: please do **not** open a public issue, see [SECURITY.md](SECURITY.md).

## Project layout

| Path | What |
| --- | --- |
| `src/main.rs` | app state, download flow (aria2 / yt-dlp / curl / ffmpeg), polling, persistence, CLI |
| `src/protocol.rs` | browser-extension message parser and file-name rules (unit-tested) |
| `src/ws.rs` | WebSocket server on `127.0.0.1:10007` (origin check) |
| `src/aria2.rs` | aria2 child process + JSON-RPC client |
| `src/jobs.rs` | external jobs (yt-dlp, curl, ffmpeg) |
| `src/ui.rs` | GTK 4 / libadwaita windows and dialogs |
| `src/platform.rs` | OS differences (folders, private files, bundled tools, notifications) |
| `src/instance.rs` | single-instance forwarding on Windows |
| `extension/` | Kara DM browser extension (Manifest V3, Firefox + Chromium) |
| `data/` | desktop file, AppStream metainfo, app icon |
| `packaging/` | Flatpak manifest, Windows bundle + installer |
| `docs/` | [architecture](docs/ARCHITECTURE.md) and [protocol](docs/PROTOCOL.md) |

## Development setup

### Linux

You need Rust (stable), GTK ≥ 4.16, libadwaita ≥ 1.6 and aria2; yt-dlp, ffmpeg and curl are optional.

```bash
# Arch
sudo pacman -S --needed rust gtk4 libadwaita aria2 yt-dlp ffmpeg
# Fedora 41+
sudo dnf install cargo gtk4-devel libadwaita-devel aria2 yt-dlp ffmpeg-free

cargo run                 # open the window (the app keeps running in the background after you close it)
cargo run -- --selftest   # check aria2 + the browser port without a window
```

Stop a running instance with `kara-dm --quit` (or `cargo run -- --quit`) before starting a new build.

### Windows

Use [MSYS2](https://www.msys2.org/) in the **UCRT64** shell:

```bash
pacman -S --needed mingw-w64-ucrt-x86_64-{rust,pkgconf,gtk4,libadwaita,librsvg,adwaita-icon-theme,aria2}
cargo run
bash packaging/windows/bundle.sh   # self-contained folder in dist/windows/kara-dm
```

### Flatpak

```bash
flatpak-builder --user --install --install-deps-from=flathub --force-clean build \
  packaging/flatpak/io.github.kamisama_v0.KaraDM.yml
flatpak run io.github.kamisama_v0.KaraDM
```

### Browser extension

```bash
cd extension
node --test test/          # unit tests (no browser needed)
node build.mjs             # -> dist/firefox, dist/chromium
npx web-ext@8 run -s dist/firefox    # try it in a temporary Firefox profile
```

For Chromium, load `extension/dist/chromium` with **Load unpacked**.

## Before you open a pull request

CI runs the same checks; please run them locally:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
(cd extension && node --test test/ && node build.mjs && npx -y web-ext@8 lint -s dist/firefox)
```

Guidelines:

- Keep changes focused; one topic per PR. Explain the *why* in the description.
- Match the surrounding code style. Comments explain intent, not the obvious.
- Add or update tests for parsing, file-name rules and other pure logic (`src/protocol.rs`,
  `extension/src/common.js`).
- Anything that touches the network surface (WebSocket server, aria2 RPC, extension permissions) needs a short
  note on the security impact.
- Changes to the extension ↔ app messages must keep compatibility with Neat Download Manager's extension and
  must update [docs/PROTOCOL.md](docs/PROTOCOL.md).
- Update [CHANGELOG.md](CHANGELOG.md) under **Unreleased**.
- Commit messages: short imperative subject (`Add speed limit per download`), details in the body.

## Translations

UI strings live in `src/ui.rs` / `src/main.rs` (app) and the HTML/JS in `extension/src/` (extension). Proper
gettext support is planned; until then, open an issue to coordinate before translating so the work isn't lost.

## Releases (maintainers)

1. Bump `version` in `Cargo.toml` (and `extension/manifest.base.json` if the extension changed), update
   `CHANGELOG.md` and `data/io.github.kamisama_v0.KaraDM.metainfo.xml`.
2. Tag: `git tag v0.x.y && git push --tags`. The [release workflow](.github/workflows/release.yml) builds the
   Flatpak, Windows installer/zip and extension zips and publishes a GitHub Release.
3. Firefox extension: sign with `extension/release-firefox.sh` (needs the maintainers' AMO API key) and attach
   the signed `.xpi` to the release.

## License

By contributing you agree that your contributions are licensed under the [Apache License 2.0](LICENSE).
