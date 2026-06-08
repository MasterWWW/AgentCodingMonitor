# Release

## 触发方式

向仓库推送 **`v*` 标签** 会触发 [`.github/workflows/release.yml`](../.github/workflows/release.yml)：

- **macOS**：在 `macos-latest` 上构建 `.dmg`（Apple Silicon）
- **Windows**：在 `windows-latest` 上构建 `.msi` / NSIS `.exe`
- 构建完成后自动创建 [GitHub Release](https://github.com/vibe-monitor/vibe-monitor/releases) 并上传安装包

## 发布步骤

1. **更新版本号**（保持一致）：
   - `apps/desktop/src-tauri/tauri.conf.json` → `version`
   - 根目录 `Cargo.toml` workspace `version`（如有需要）

2. **提交并打标签**：

```bash
git tag v0.1.1
git push origin v0.1.1
```

3. 打开 **GitHub → Actions → Release**，等待 workflow 完成。

4. 在 **Releases** 页面下载对应平台的安装包。

## 产物说明

| 平台 | 典型文件 |
|------|----------|
| macOS | `Vibe Monitor_<version>_aarch64.dmg` |
| Windows | `Vibe Monitor_<version>_x64-setup.exe`、`.msi` |

`tauri.conf.json` 中 `bundle.targets` 为 `all`，Windows 可能同时产出 NSIS 与 MSI。

## 代码签名（可选）

当前 workflow **未配置签名**，产物为未签名构建：

- **macOS**：首次打开可能需右键 → 打开，或 `xattr -cr` 去除隔离属性
- **Windows**：SmartScreen 可能提示未知发布者

后续可在 workflow 中注入密钥：

| 平台 | 常见 Secrets |
|------|----------------|
| macOS | `APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_SIGNING_IDENTITY`、`APPLE_ID`、`APPLE_PASSWORD`、`APPLE_TEAM_ID` |
| Windows | `WINDOWS_CERTIFICATE`、`WINDOWS_CERTIFICATE_PASSWORD` |

## 本地构建

若需在本机打包，见 [README.md](../README.md)「从源码构建」章节。
