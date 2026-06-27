package com.vibemonitor.bridge.watch

/**
 * 向 vivo Watch 发送 `action_prompt`、接收 `action_response`。
 * 实现类在接入 vivo 智能终端设备 SDK 后替换 [VivoWatchRelay]。
 */
interface WatchRelay {
    val name: String
    fun isAvailable(): Boolean
    fun start(onResponseJson: (String) -> Unit)
    fun stop()
    fun sendPrompt(envelopeJson: String)
}
