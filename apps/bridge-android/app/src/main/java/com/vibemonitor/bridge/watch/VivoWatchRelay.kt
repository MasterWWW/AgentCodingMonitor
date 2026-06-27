package com.vibemonitor.bridge.watch

import android.util.Log
import org.json.JSONObject

/**
 * vivo 智能终端设备 SDK 接入点（与 BlueOS [connectionManager] 配对）。
 *
 * 手机端 DeviceRpc 发送/接收的 `data` 字段应与手表 `sendToPhone` / `onmessage` 的
 * JSON 信封一致（`type: action_prompt | action_response`）。
 *
 * 集成步骤见 `apps/bridge-android/README.md` 与 `apps/watch-blueos/README.md`。
 */
class VivoWatchRelay : WatchRelay {
    override val name: String = "vivo DeviceRpc"

    override fun isAvailable(): Boolean =
        runCatching { Class.forName("com.vivo.health.device.rpc.DeviceRpcManager") }.isSuccess

    private var callback: ((String) -> Unit)? = null

    override fun start(onResponseJson: (String) -> Unit) {
        callback = onResponseJson
        if (!isAvailable()) {
            Log.w(TAG, "vivo SDK not integrated — watch replies won't arrive via RPC")
            return
        }
        // TODO: DeviceRpcManager.getInstance().init(application, encryStr, callback)
        // TODO: setMessageListener { raw ->
        //   val parsed = WatchMessageParser.parseResponse(raw)
        //     ?: WatchMessageParser.parseResponseMap(raw as Map<*, *>)
        //   if (parsed != null) onResponseJson(buildEnvelope(parsed))
        // }
    }

    override fun stop() {
        callback = null
    }

    override fun sendPrompt(envelopeJson: String) {
        if (!isAvailable()) return
        runCatching {
            val obj = JSONObject(envelopeJson)
            val payload = obj.toMap()
            // TODO: DeviceRpcManager.getInstance().sendRequest(payload)
            Log.d(TAG, "sendPrompt → watch: $payload")
        }.onFailure { e ->
            Log.w(TAG, "sendPrompt failed", e)
        }
    }

    private fun buildEnvelope(parsed: WatchMessageParser.WatchResponse): String {
        return JSONObject()
            .put("type", "action_response")
            .put("id", parsed.actionId)
            .put("ts", System.currentTimeMillis() / 1000)
            .put(
                "data",
                JSONObject()
                    .put("action_id", parsed.actionId)
                    .put("choice", parsed.choice)
                    .put("from", parsed.from),
            )
            .toString()
    }

    private fun JSONObject.toMap(): Map<String, Any?> {
        val map = mutableMapOf<String, Any?>()
        keys().forEach { key ->
            map[key] = get(key)
        }
        return map
    }

    companion object {
        private const val TAG = "VivoWatchRelay"
    }
}
