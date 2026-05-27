# AGENTS.md

- 需求不明确时，优先从当前打开文件及同目录文件推断上下文。
- 代码段修改后，保持原有的代码风格和格式。
- 多文件关联修改需保持一致；架构说明见 `docs/architecture.md`。
- 计划文档归档在 `docs/plans/`。

## Cursor Cloud specific instructions

### Project overview

Vibe Monitor is a cross-platform Tauri 2 desktop app (Rust + React/TypeScript) that monitors AI coding assistants. No external services (databases, Docker, cloud APIs) are required.

### Key commands

| Task | Command |
|------|---------|
| Install frontend deps | `cd apps/desktop && npm ci` |
| Build Rust crates | `cargo build -p vibe-hook -p vibe-core` |
| Run Rust tests | `cargo test -p vibe-core` |
| Build frontend (lint/typecheck) | `cd apps/desktop && npm run build` |
| Full workspace check | `cargo check --workspace` |
| Vite dev server (frontend only) | `cd apps/desktop && npm run dev` |
| Full desktop dev mode | `cd apps/desktop && npm run tauri dev` |

### Caveats

- **Tauri desktop GUI** (`npm run tauri dev`) requires a display server. In headless Cloud Agent environments, use `cargo check --workspace` and `npm run build` to verify compilation. The vibe-core HTTP server can be tested standalone without a GUI.
- **vibe-hook binary** reads from **stdin** and requires `--source cursor|claude|codex`. Example: `echo '{"hook_event_name":"preToolUse"}' | ./target/debug/vibe-hook --source cursor`
- **System deps for Linux** (needed once per fresh VM): `sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev pkg-config build-essential libxdo-dev`
- The embedded HTTP server (vibe-core) defaults to port **17392**; test with `curl http://127.0.0.1:17392/api/status`.
- One minor `dead_code` warning in `apps/desktop/src-tauri/src/lib.rs` (`hook_search_hints` field) is known and harmless.
