package com.vibemonitor.bridge.watch

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * 解析手表经 BlueXlink / DeviceRpc 回传的 JSON（与 `vibe-protocol` 对齐）。
 */
object WatchMessageParser {
    private val json = Json { ignoreUnknownKeys = true }

    data class WatchResponse(
        val actionId: String,
        val choice: String,
        val from: String,
    )

    fun parseResponse(raw: String): WatchResponse? {
        return runCatching {
            val root = json.parseToJsonElement(raw).jsonObject
            val data = (root["data"] as? JsonObject) ?: root
            val actionId = data["action_id"]?.jsonPrimitive?.contentOrNull
                ?: data["actionId"]?.jsonPrimitive?.contentOrNull
            val choice = data["choice"]?.jsonPrimitive?.contentOrNull
            val from = data["from"]?.jsonPrimitive?.contentOrNull ?: "watch"
            if (actionId.isNullOrBlank() || choice.isNullOrBlank()) return null
            WatchResponse(actionId, choice, from)
        }.getOrNull()
    }

    fun parseResponseMap(data: Map<*, *>): WatchResponse? {
        val actionId = data["action_id"]?.toString() ?: data["actionId"]?.toString()
        val choice = data["choice"]?.toString()
        val from = data["from"]?.toString() ?: "watch"
        if (actionId.isNullOrBlank() || choice.isNullOrBlank()) return null
        return WatchResponse(actionId, choice, from)
    }
}
