# Vibe Watch (BlueOS)

vivo Watch 5 交互 App：接收手机 Vibe Bridge 转发的 `action_prompt`，展示 **允许 / 拒绝**，回传 `action_response`。

## 功能

| 能力 | 说明 |
|------|------|
| 手机互联 | `connectionManager` → 包名 `com.vibemonitor.bridge` |
| 确认页 | 标题 + 正文 + 两个大按钮（圆形屏友好） |
| 协议 | 与 `crates/vibe-protocol` / 手机端 JSON 信封一致 |
| 离线缓存 | `system.storage` 保存未处理 `action_prompt` |

## 环境要求

- **BlueOS Studio**（[下载](https://developers-watch.vivo.com.cn/reference/quickstart/quick-start)）
- vivo Watch 5（建议真机调试；模拟器可预览 UI）
- 手机已安装 **Vibe Bridge** 且桥接服务运行中
- 手机安装 **vivo 运动健康**

## 工程结构

```
apps/watch-blueos/
  manifest.json           # 包名、features、路由
  package.json
  src/
    index.ux              # 待机 / 监听手机消息
    prompt.ux             # Yes/No 确认页
    common/
      bridge.js           # connectionManager 封装
      protocol.js         # action_prompt / action_response
      storage.js          # 待处理动作缓存
  assets/
    icon.png              # 114×114（需自行添加，见 assets/README.md）
```

## 配置

`manifest.json` → `customData`：

```json
{
  "phonePackage": "com.vibemonitor.bridge",
  "phoneFingerprint": ""
}
```

若手机 Bridge 使用签名证书，`phoneFingerprint` 填 vivo 文档要求的 SHA256 指纹（可选，视 SDK 要求）。

手表包名：`com.vibemonitor.watch`（与手机包名不同）。

## 构建与安装

1. 用 **BlueOS Studio** 打开 `apps/watch-blueos`
2. 点击「安装依赖」→「重新启动编译」
3. 连接 Watch 5 真机或模拟器
4. 打包 **debug rpk** → 侧载到手表，或上传 vivo 开放平台

## 消息流

```
PC waiting_user
  → 手机 SSE action_created
  → 手机 DeviceRpc / 互联 send action_prompt
  → 手表 onmessage → 跳转 prompt.ux
  → 用户点允许/拒绝
  → 手表 send action_response
  → 手机 → PC POST respond
  → PC 通知 + 剪贴板 y/n
```

### action_prompt（手机 → 手表）

```json
{
  "type": "action_prompt",
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "ts": 1719494400,
  "data": {
    "source": "cursor",
    "session_id": "abc",
    "phase": "waiting_user",
    "title": "允许执行命令？",
    "body": "npm run test",
    "actions": [
      { "id": "approve", "label": "允许" },
      { "id": "deny", "label": "拒绝" }
    ],
    "expires_at": "2026-06-27T12:05:00Z"
  }
}
```

### action_response（手表 → 手机）

```json
{
  "type": "action_response",
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "ts": 1719494401,
  "data": {
    "action_id": "550e8400-e29b-41d4-a716-446655440000",
    "choice": "approve",
    "from": "watch"
  }
}
```

## 手机端配套

手机需完成 `device-rpc.aar` 接入后，`VivoWatchRelay` 才能把消息送到手表。未完成前，手机通知栏兜底仍可将回执发回 PC。

详见 `apps/bridge-android/README.md`。

## 调试建议

1. 先在手机 Vibe Bridge 确认「已连接开发机」
2. 手表打开 Vibe确认，应显示「已连接手机」
3. 在 PC 触发 `waiting_user`，观察手表是否进入确认页
4. BlueOS Studio DevTools 查看 `console.log`

## 相关文档

- `docs/plans/watch-companion.md`
- [穿戴业务 Kit](https://developers-watch.vivo.com.cn/api/connect/interconnect/)
