# Vibe Bridge (Android)

手机桥接 App：通过 mDNS 连接开发机 `vibe-core`，将 Agent 待确认动作转发到 vivo Watch 5。

## 状态

Rust 端（`vibe-core` action API + mDNS + 桌面托盘）已实现。本目录为 Android 工程占位，待接入：

- vivo 智能终端设备 SDK (`DeviceRpcManager`)
- `NsdManager` mDNS 发现 `_vibe-monitor._tcp`
- OkHttp SSE `GET /api/watch/stream`
- `POST /api/actions/{id}/respond`

## 配对

扫描桌面托盘「手表配对二维码」，JSON 格式见 `docs/plans/watch-companion.md`。

## 开发

- 语言：Kotlin + Jetpack Compose
- 最低 SDK：按 vivo SDK 要求（实现时确认）
- 包名建议：`com.vibemonitor.bridge`
