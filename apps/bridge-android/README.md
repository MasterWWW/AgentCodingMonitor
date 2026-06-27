# Vibe Bridge (Android)

vivo X300 手机桥接 App：扫码配对 → mDNS 发现开发机 → SSE 同步 Agent 待确认 → 转发手表（或手机通知兜底）。

## 功能

| 能力 | 状态 |
|------|------|
| 扫描/粘贴托盘配对 JSON | ✅ |
| mDNS 发现 `_vibe-monitor._tcp` | ✅ |
| SSE `GET /api/watch/stream` | ✅ |
| 回传 `POST /api/actions/{id}/respond` | ✅ |
| 手机通知 Yes/No 兜底 | ✅ |
| vivo DeviceRpc → Watch 5 | 🔌 需放入 `app/libs/device-rpc.aar` |

## 环境要求

- Android Studio Ladybug+ / JDK 17
- vivo X300，OriginOS
- 与开发机 **同一 WiFi**
- 开发机 Vibe Monitor 已启用 **手表伴侣**（独立于 iPad 看板）

## 构建

```bash
cd apps/bridge-android
# 首次：用 Android Studio 打开并同步 Gradle，或：
./gradlew assembleDebug
```

安装：

```bash
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

## 使用

1. 开发机托盘 → 启用 **手表伴侣** → **显示手表配对二维码**
2. 手机打开 Vibe Bridge → 扫描二维码（或粘贴 JSON）
3. 点击 **启动桥接服务**
4. Agent 进入 `waiting_user` 后：
   - 已接 vivo SDK：转发到手表
   - 未接 SDK：手机通知栏点 **允许/拒绝**（仍会回传 PC，并触发剪贴板 `y`/`n`）

## 配对 JSON 示例

```json
{
  "v": 1,
  "device_id": "a1b2c3",
  "service": "vibe-monitor-a1b2c3._tcp.local",
  "host_fallback": null,
  "port": 17392,
  "token": "..."
}
```

## vivo SDK 接入

1. [vivo 开放平台](https://dev.vivo.com.cn/) 创建应用，获取 AppID 与智能终端 SDK 密钥
2. 下载 `device-rpc.aar` → `app/libs/`
3. `app/build.gradle.kts` 添加 `implementation(files("libs/device-rpc.aar"))`
4. 按官方文档配置 `AndroidManifest` meta-data
5. 完善 `watch/VivoWatchRelay.kt` 中的 `DeviceRpcManager` 调用
6. 手机安装 **vivo 运动健康**

## OriginOS 建议

- 允许 **自启动**、**后台运行**、**电池优化无限制**
- 授予 **通知**、**相机**（扫码）权限

## 工程结构

```
app/src/main/java/com/vibemonitor/bridge/
  data/PairingStore.kt          # DataStore 持久化配对
  discovery/                    # mDNS + EndpointResolver
  network/                      # OkHttp SSE + REST
  service/BridgeService.kt      # 前台服务主循环
  watch/                        # WatchRelay 抽象 + vivo 接入点
  ui/                           # Compose 界面
```

## 相关文档

- `docs/plans/watch-companion.md`
- `crates/vibe-protocol` — 消息格式
