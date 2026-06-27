package com.vibemonitor.bridge.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class PairingConfig(
    val v: Int = 1,
    @SerialName("device_id") val deviceId: String,
    val service: String,
    @SerialName("host_fallback") val hostFallback: String? = null,
    val port: Int,
    val token: String,
)

@Serializable
data class ResolvedEndpoint(
    val host: String,
    val port: Int,
    val baseUrl: String,
)

@Serializable
data class ActionButton(
    val id: String,
    val label: String,
    val style: String? = null,
)

@Serializable
data class PendingAction(
    val id: String,
    val created_at: String? = null,
    val source: String,
    val session_id: String,
    val phase: String,
    val title: String,
    val body: String,
    val actions: List<ActionButton>,
    val expires_at: String,
)

@Serializable
data class WatchStreamEvent(
    val event: String,
    val data: kotlinx.serialization.json.JsonElement,
)

@Serializable
data class ActionResponseBody(
    @SerialName("action_id") val actionId: String,
    val choice: String,
    val from: String = "phone",
)

@Serializable
data class Envelope<T>(
    val type: String,
    val id: String,
    val ts: Long,
    val data: T,
)

@Serializable
data class CancelledPayload(
    val id: String,
    val reason: String? = null,
)

enum class ConnectionPhase {
    Idle,
    Discovering,
    Connecting,
    Connected,
    Error,
}

data class BridgeUiState(
    val phase: ConnectionPhase = ConnectionPhase.Idle,
    val message: String = "未配对",
    val pairing: PairingConfig? = null,
    val endpoint: ResolvedEndpoint? = null,
    val serviceRunning: Boolean = false,
    val pendingCount: Int = 0,
)
