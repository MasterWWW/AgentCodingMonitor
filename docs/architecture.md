# Architecture

## Overview

Vibe Monitor uses a **single desktop process** that embeds `vibe-core` (Axum HTTP server on localhost) and a **Tauri + React** UI. External AI tools invoke the **`vibe-hook`** binary from user-level hook configs; the binary POSTs normalized events to `POST /api/events`.

```mermaid
flowchart LR
  Tools[Cursor Claude Codex]
  Hook[vibe-hook]
  Core[vibe-core]
  UI[Tauri UI]

  Tools --> Hook
  Hook -->|POST /api/events| Core
  UI -->|SSE /api/stream| Core
```

## State machine

- Events map to phases: `active`, `idle` (30s timeout), `waiting_user`, `stopped`, `unknown`.
- Sessions keyed by `source:session_id`.
- Lite mode recursively watches all three transcript trees (any `*.jsonl` under each root): `~/.cursor/projects`, `~/.claude/projects`, `~/.codex/projects` — none skipped.

## Desktop presentation (HUD)

Persisted in `state.json` as `display` (独立开关，可任意组合)：

| 开关 | 行为 |
|------|------|
| `float_hud` | 透明置顶 HUD 浮窗；macOS 默认开 |
| `tray_status` | 托盘相位图标 + 菜单栏标题（Active/WaitingUser）；默认开 |
| `lan_companion.enabled` | 局域网 iPad 看板；默认关 |

旧版互斥字段 `presentation`（`float` / `menubar`）在读取时自动迁移。

macOS additionally:

- `visibleOnAllWorkspaces` on the main window so the float HUD follows all Spaces.
- `LSUIElement` + `ActivationPolicy::Accessory` so the app does not appear in the Dock.

Tray menu toggles each display channel; `get_display_settings` / `set_display_*` Tauri commands expose the same preferences (`get_presentation` 仍兼容旧互斥模式)。

## Hook installation

`install::install_hooks` merges entries tagged `metadata.source = "vibe-monitor"` into:

- `~/.cursor/hooks.json`
- `~/.claude/settings.json` → `hooks`
- `~/.codex/hooks.json`
- Enables `[features] codex_hooks = true` in `~/.codex/config.toml` when missing

Windows installs `vibe-hook.cmd` wrapping `vibe-hook.exe`.

## API

| Method | Path |
|--------|------|
| GET | `/api/status` |
| GET | `/api/stream` (SSE) |
| POST | `/api/events` |
| POST | `/api/install-hooks` |
| GET | `/api/doctor` |
| GET | `/api/lan-info` (loopback only) | 局域网看板配对信息 |
| GET | `/mobile` | iPad / 手机看板页 |

### LAN companion（iPad 看板）

可选在托盘启用：绑定 `0.0.0.0`，局域网设备通过 `GET /mobile?token=…` 查看看板；`POST` 等写接口仍仅 `127.0.0.1`。详见 `docs/plans/lan-companion-dashboard.md`。

## Data directory

Resolved via `directories` crate as `ProjectDirs::from("com", "VibeMonitor", "vibe-monitor")` data dir:

- `bin/vibe-hook` — installed reporter
- `port` — current HTTP port
- `state.json` — reserved for future persistence
- `first-run.done` — wizard completion marker
