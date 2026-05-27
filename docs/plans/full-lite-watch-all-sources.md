# 轻量模式：三端全量监视

归档日期：2026-05-27

## 需求

所有来源都要监视，不能跳过（Cursor / Claude Code / Codex）。

## 改动

| 文件 | 说明 |
|------|------|
| `paths.rs` | `codex_transcripts_root`、`lite_watch_roots()` |
| `lite.rs` | 按监视根前缀映射来源；Codex 纳入；fallback 路径匹配 |
| `install.rs` | 三端 hook 任一失败则 `ok: false` |
| `vibe-hook` | 与 `event.rs` 对齐字段；POST 失败非 0 退出 |
| `api.rs` / `server.rs` / `lib.rs` | 安装 hook 时传入完整 search hints |
| `lib.rs` | 托盘「当前」与 HUD 同用 `pick_display_source` |

## 行为

- 轻量开启：监视 `~/.cursor/projects`、`~/.claude/projects`、`~/.codex/projects` 下所有 `.jsonl`。
- 轻量关闭：仍靠三端 hook 上报，安装需三端均成功才算 `ok`。
