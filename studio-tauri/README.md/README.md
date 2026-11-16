# PaperPilot Studio — Tauri Edition

The same PaperPilot Studio UI and feature set as the Electron edition
(`../studio`), rebuilt on **Tauri 2 + Rust**:

- **~6–10 MB download** instead of ~109 MB — no bundled Chromium; the app
  renders with the operating system's own webview (WebView2 on Windows).
- The built-in PaperPilot engine (RCON client, the 13 tools, tier checks,
  command allowlist, JSONL audit log) is a **Rust port** of the TypeScript
  core that ships in this repo — same tool names, same output text, same
  safety behaviour. The MCP server itself stays TypeScript; this port only
  covers the engine the Studio embeds.
- **Motion** (`motion/react`) animations throughout: spring page
  transitions, a sliding active-nav marker, animated modals, a springy user
  switcher and staggered tool cards.

Everything else is shared with the Electron edition: the same React UI, the
same six themes, the same team roles (owner / admin / operator / viewer
mapped to the admin / operator / readonly tool tiers), the same settings
file format (existing settings are imported automatically on first run),
and the same audit log format.

## Develop

```bash
npm install
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

Windows releases are built by GitHub Actions (`.github/workflows/
studio-tauri.yml` in the repo root) on `windows-latest` — Tauri cannot
cross-compile a Windows binary from Linux/macOS because it links against
the platform webview.

## Layout

- `src/renderer/` — the Studio React UI (shared with the Electron edition)
- `src/preload/` — the `window.studio` bridge, implemented over Tauri
  `invoke()` commands
- `src-tauri/` — the Rust shell + engine port
  - `rcon.rs` — Source RCON protocol client (multi-packet reassembly)
  - `tools.rs` — the 13 tools, output parsers, tier/allowlist guards
  - `service.rs` — the in-process service (team gate, status parsing,
    backups, audit reader, GitHub stars)
  - `settings.rs` / `audit.rs` — settings store and the JSONL audit log
