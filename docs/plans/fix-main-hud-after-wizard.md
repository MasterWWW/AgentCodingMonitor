# 修复：向导完成后主浮窗不显示 HUD

归档日期：2026-05-27

## 问题

- `main` 窗口（120×36）在首次启动时因 `needs_first_run` 也渲染了 `WizardApp`，内容挤在浮窗里不可见。
- 用户在 `wizard` 窗口点「启用监听」后，`main` 的 React 状态仍为 wizard，未切换到 `MainApp`（圆点 + 工具名）。

## 改动

| 文件 | 说明 |
|------|------|
| `App.tsx` | `main` 仅 `MainApp`；`wizard` 仅 `WizardApp`；监听 `first-run-complete` |
| `lib.rs` | 首次启动隐藏 `main`；`finish_first_run` 写标记、`apply_presentation`、显示并 focus `main`、发事件 |

## 验证

1. 删除 `~/Library/Application Support/com.VibeMonitor.vibe-monitor/first-run.done` 模拟首次安装。
2. 启动 App → 仅大向导窗口，浮窗隐藏。
3. 启用监听 → 向导关闭，浮窗出现「Cursor」+ 状态圆点。
