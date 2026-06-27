package com.vibemonitor.bridge.watch

import android.util.Log

/**
 * 开发占位：仅打日志。真机请换 [VivoWatchRelay] 并放入 `app/libs/device-rpc.aar`。
 */
class LogWatchRelay : WatchRelay {
    override val name: String = "日志占位"

    override fun isAvailable(): Boolean = true

    private var callback: ((String) -> Unit)? = null

    override fun start(onResponseJson: (String) -> Unit) {
        callback = onResponseJson
        Log.i(TAG, "started (no watch hardware)")
    }

    override fun stop() {
        callback = null
    }

    override fun sendPrompt(envelopeJson: String) {
        Log.i(TAG, "sendPrompt: $envelopeJson")
    }

    companion object {
        private const val TAG = "LogWatchRelay"
    }
}
