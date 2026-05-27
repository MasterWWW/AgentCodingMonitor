# 主浮窗精简 + 托盘菜单

归档日期：2026-05-25（2026-05-25 更新：文本自适应宽 + phase 主题）

## 职责划分

| 区域 | 内容 |
|------|------|
| **主浮窗 HUD** | **单行**：信号灯 + 当前展示的工具名 |
| **托盘右键菜单** | 默认展示提示、三行只读状态、修复/诊断/轻量、设为默认工具、显示/隐藏/退出 |

## HUD 展示规则（单工具）

1. 存在 `active` 或 `waiting_user` 的 session 时，取 `last_activity_at` 最新者。
2. 否则取任意 phase 下 `last_activity_at` 最新的 session（含 `stopped` / `idle`），任务结束后仍显示最近一次活跃的工具。
3. 再无 session 时按各来源 `last_seen`；最后才用 `default_source`。
4. 托盘菜单可切换默认工具（「设为默认 · …」），仅在上面的回退链末尾生效。

## 窗口尺寸

- `body.hud-mode` + `#root`：`fit-content`，避免撑满 `tauri.conf` 初始宽。
- `.app`：`max-content` / `inline-flex`；`useMainWindowAutoSize` 按 `offsetWidth/offsetHeight` 同步窗口。
- `tauri.conf` `main` 初始约 `120×36`，首帧占位；`Cursor` 窄于 `Claude Code`。
- 切换 `displaySource` / `phase` 时 `ResizeObserver` 重新测量。

## 状态主题（phase-*）

| 类名 | phase | 视觉 |
|------|-------|------|
| `phase-active` | active | 翠绿渐变、绿描边、圆点脉冲 |
| `phase-waiting` | waiting_user | 琥珀渐变、慢脉冲 |
| `phase-stopped` | stopped | 红粉渐变、静态警示 |
| `phase-idle` | idle | 蓝灰冷色 |
| `phase-unknown` | unknown | 紫灰虚线边框 |

## 全窗拖拽（仅移动）

- 必须使用 `data-tauri-drag-region="deep"`：空字符串仅对**直接点击**该元素有效，点到 `.label` / `.dot` 等子节点不会拖动。
- `MainApp` 挂载时：`body` / `html` / `#root` / `.app` 均设 `deep`；卸载时移除。
- CSS：`body.hud-mode` 与子孙 `-webkit-app-region: drag`（辅助）；`user-select: none`。
- `contextmenu` 在 HUD 上 `preventDefault`；主浮窗无 `onClick` 等业务逻辑。
- 点文字、圆点、面板任意处均可拖动；交互（改状态、菜单）走托盘。

## 窗口圆角

- macOS：`set_effects` + `radius: 12` 与 CSS `.app` 圆角一致。
- `shadow: false`，避免直角黑框。

## 实现要点

- [`App.tsx`](../apps/desktop/src/App.tsx)：`hud-mode`、`phase-${dotClass(phase)}`
- [`styles.css`](../apps/desktop/src/styles.css)：渐变底、光晕、标签色、脉冲动画
- [`lib.rs`](../apps/desktop/src-tauri/src/lib.rs)：`TRAY_ID` 托盘、`build_tray_menu`、SSE 广播刷新菜单、`rfd` 弹窗
- [`useMainWindowAutoSize.ts`](../apps/desktop/src/useMainWindowAutoSize.ts)：宽高随 `.app` 外框

## 验证

1. HUD 宽度随工具名变化，无多余空白
2. 五种 phase 底色/边框/圆点/文字差异明显
3. 整窗任意位置可拖、无法选中文字、右键无菜单
4. 托盘右键可见状态行与操作项
5. Agent 活动后 HUD 灯与托盘文案同步更新
