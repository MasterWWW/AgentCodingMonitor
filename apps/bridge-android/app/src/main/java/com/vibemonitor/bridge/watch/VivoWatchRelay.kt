package com.vibemonitor.bridge.watch

import android.content.Context
import android.util.Log

/**
 * vivo 智能终端设备 SDK 接入点。
 *
 * 1. 从 vivo 开放平台下载 `device-rpc.aar` 放到 `app/libs/`
 * 2. 在 `app/build.gradle.kts` 添加 `implementation(files("libs/device-rpc.aar"))`
 * 3. 在 AndroidManifest 配置 appid / SDK 密钥（见 vivo 文档）
 * 4. 将 [BridgeRuntime] 中的 relay 切换为本类
 *
 * 当前未内置 AAR，[isAvailable] 为 false。
 */
class VivoWatchRelay(private val context: Context) : WatchRelay {
    override val name: String = "vivo DeviceRpc"

    override fun isAvailable(): Boolean =
        runCatching { Class.forName("com.vivo.health.device.rpc.DeviceRpcManager") }.isSuccess

    private var callback: ((String) -> Unit)? = null

    override fun start(onResponseJson: (String) -> Unit) {
        callback = onResponseJson
        if (!isAvailable()) {
            Log.w(TAG, "vivo SDK not integrated — see apps/bridge-android/README.md")
            return
        }
        // TODO: DeviceRpcManager.getInstance().init(...)
        // TODO: register receive callback → onResponseJson(json)
    }

    override fun stop() {
        callback = null
    }

    override fun sendPrompt(envelopeJson: String) {
        if (!isAvailable()) return
        // TODO: DeviceRpcManager.getInstance().sendRequest(envelopeJson)
        Log.d(TAG, "sendPrompt pending SDK wiring: $envelopeJson")
    }

    companion object {
        private const val TAG = "VivoWatchRelay"
    }
}
