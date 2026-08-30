# Third-Party Licenses & Notices

This document lists the licenses of direct dependencies bundled with **GitManager**.  
GitManager itself is licensed under the **MIT License** (see `LICENSE`). All listed dependencies use permissive licenses compatible with MIT for binary distribution. No copyleft code is linked in a way that imposes source-disclosure on the combined work — `libgit2` is used under its **GPL-2.0 with linking exception**, which explicitly permits linking with non-GPL applications.

> To generate a full up-to-date list locally, run:
> ```bash
> cargo install cargo-license
> cargo license --direct-deps-only --json > licenses.json
> # or
> cargo deny check licenses
> ```

## Direct Rust Crates (Cargo)

| Crate | Version (at release) | License | Notes |
|-------|----------------------|---------|-------|
| `eframe` | 0.32 | MIT OR Apache-2.0 | egui framework, glow backend |
| `egui` | 0.32 | MIT OR Apache-2.0 | Immediate-mode GUI |
| `egui_extras` | 0.32 | MIT OR Apache-2.0 | SVG image loader |
| `git2` | 0.20 | MIT OR Apache-2.0 | Safe bindings to libgit2 |
| `libgit2-sys` (transitive) | — | GPL-2.0 **with linking exception** | Vendored C library; exception allows linking without GPL contamination (see https://github.com/libgit2/libgit2/blob/main/COPYING) |
| `libz-sys` (transitive) | — | MIT OR Apache-2.0 / Zlib | Compression |
| `openssl-sys` / `vendored-openssl` (transitive) | — | Apache-2.0 | Vendored OpenSSL, only when `vendored-openssl` feature is enabled |
| `serde` | 1.0 | MIT OR Apache-2.0 | Serialization |
| `serde_json` | 1.0 | MIT OR Apache-2.0 | JSON |
| `directories` | 6.0 | MIT OR Apache-2.0 | Platform config dirs (`%APPDATA%` et al.) |
| `rfd` | 0.15 | MIT OR Apache-2.0 | Native file dialogs |
| `walkdir` | 2.5 | MIT OR Unlicense | Directory walking |
| `rayon` | 1.11 | MIT OR Apache-2.0 | Parallel iterators |
| `anyhow` | 1.0 | MIT OR Apache-2.0 | Error handling |
| `image` | 0.25 | MIT OR Apache-2.0 | ICO/PNG decoding for window icon (features `png`, `ico` only) |
| `winres` (build) | 0.1 | MIT | Windows resource embedding (icon + manifest) |
| `tempfile` (dev) | 3.15 | MIT OR Apache-2.0 | Tests |

For a complete transitive closure, see `Cargo.lock` and run `cargo tree`.

## Icon Assets (`assets/icons/`)

| Asset | Source | License | Trademark |
|-------|--------|---------|-----------|
| `folder.svg` | [Phosphor Icons](https://github.com/phosphor-icons/core) | **MIT** | — |
| `vscode.svg`, `visualstudio.svg`, `rider.svg`, `claude.svg`, `codex.svg`, `gemini.svg`, `copilot.svg`, `cursor.svg`, `aider.svg` | [Simple Icons](https://github.com/simple-icons/simple-icons) | **CC0 1.0 Universal** (collection) | Individual icons are trademarks: VS Code / Visual Studio (Microsoft), Rider (JetBrains), Claude (Anthropic), Codex (OpenAI), Gemini (Google), Copilot (GitHub/Microsoft), Cursor (Cursor Inc.), Aider (Paul Gauthier). Use under **nominative fair use** — see `assets/icons/LICENSE.txt`. |

Trademark guidelines:
- VS Code: https://code.visualstudio.com/brand
- Visual Studio: https://visualstudio.microsoft.com/license-terms/
- JetBrains: https://www.jetbrains.com/company/brand/
- Anthropic: https://www.anthropic.com/brand

## Windows SDK / Toolchain (CI only)

- `mingw-w64`, `clang`, `lld`, `cargo-xwin` are used only at build time in Docker/CI. They are not redistributed with the `.exe`. The resulting binary statically links `libgit2` + `libssl` via vendored features and contains the embedded Windows manifest + icon (produced by `winres`).

## Summary

- **Project license:** MIT — permissive, no attribution beyond preserving `LICENSE`.
- **Dependencies:** All use MIT/Apache-2.0/CC0/Unlicense/Zlib, except `libgit2` which uses GPL-2.0 **with linking exception** — compatible with MIT for this use case.
- **Fonts:** `egui` uses built-in `Hack`/`Noto` fonts under their respective permissive licenses (MIT/OFL).
- **No GPL copyleft is triggered** for end users of the prebuilt `gitmanager.exe`. If you modify `libgit2` itself, consult its `COPYING` file.

If you distribute a fork, please retain:
- `LICENSE` (MIT)
- `assets/icons/LICENSE.txt`
- This file (`THIRD_PARTY_LICENSES.md`) or an equivalent notice
- And respect the trademark guidelines linked above.
