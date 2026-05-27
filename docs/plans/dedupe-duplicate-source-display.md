# 修复：同 session_id 重复来源导致 HUD 显示 Claude Code

归档日期：2026-05-27

## 问题

同一 Cursor 任务被 `vibe-hook --source cursor` 与 `vibe-hook --source claude` 各上报一次，`session_id` 相同，`claude_code` 仅晚几十微秒。原逻辑按 `last_activity_at` 取最新，HUD 误显示 Claude Code。

本机可能未主动使用 Claude Code，但「启用监听」会写入 `~/.claude/settings.json`，Cursor 环境仍可能触发 Claude 风格 hook。

## 策略

- **不跳过**任何来源的 hook / 轻量监视。
- 展示层对同一 `session_id`、2 秒内多来源重复上报做稳定选择：
  1. `default_source`
  2. Cursor
  3. Claude Code
  4. Codex
- 非近时间重复组仍按组内最新 `last_activity_at`；多组之间仍选最新组。

## 改动

| 文件 | 说明 |
|------|------|
| `crates/vibe-core/src/state.rs` | `pick_display_source` / `newest_session_matching` |
| `apps/desktop/src/pickHudSource.ts` | 与后端一致的 HUD 规则 |

## 验证

```bash
cargo test -p vibe-core state::tests
cd apps/desktop && npm run build
```

手动：同一 `session_id` 下 cursor + claude_code 均为 active 时，`curl /api/status` 后 HUD 应显示 Cursor（默认来源为 Cursor 时）。
