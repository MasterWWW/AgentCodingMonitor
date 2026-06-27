# 将 vivo 开放平台下载的 `device-rpc.aar` 放在此目录，并在 `app/build.gradle.kts` 添加：

```kotlin
implementation(files("libs/device-rpc.aar"))
```

未放置 AAR 时，App 使用手机通知作为确认兜底（仍可回传 PC）。
