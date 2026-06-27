package com.vibemonitor.bridge

import android.content.Context
import com.vibemonitor.bridge.model.PairingConfig
import com.vibemonitor.bridge.model.ResolvedEndpoint
import com.vibemonitor.bridge.network.VibeApiClient
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * 进程内共享状态，供 [service.BridgeService] 与通知快捷操作使用。
 */
object BridgeRuntime {
    private val mutex = Mutex()

    var pairing: PairingConfig? = null
        private set
    var endpoint: ResolvedEndpoint? = null
        private set

    val api = VibeApiClient()

    private val _state = MutableStateFlow(BridgeRuntimeState())
    val state: StateFlow<BridgeRuntimeState> = _state.asStateFlow()

    suspend fun updatePairing(config: PairingConfig?) {
        mutex.withLock { pairing = config }
        _state.value = _state.value.copy(pairing = config)
    }

    suspend fun updateEndpoint(ep: ResolvedEndpoint?) {
        mutex.withLock { endpoint = ep }
        _state.value = _state.value.copy(endpoint = ep)
    }

    fun updatePhase(phase: String, message: String, pending: Int = _state.value.pendingCount) {
        _state.value = _state.value.copy(
            phase = phase,
            message = message,
            pendingCount = pending,
        )
    }

    fun setServiceRunning(running: Boolean) {
        _state.value = _state.value.copy(serviceRunning = running)
    }

    fun incrementPending() {
        _state.value = _state.value.copy(pendingCount = _state.value.pendingCount + 1)
    }

    fun decrementPending() {
        _state.value = _state.value.copy(
            pendingCount = (_state.value.pendingCount - 1).coerceAtLeast(0),
        )
    }

    suspend fun respond(actionId: String, choice: String, from: String): Boolean {
        val ep = mutex.withLock { endpoint }
        val cfg = mutex.withLock { pairing }
        if (ep == null || cfg == null) return false
        return api.respond(ep, cfg.token, actionId, choice, from)
    }
}

data class BridgeRuntimeState(
    val phase: String = "Idle",
    val message: String = "未配对",
    val pairing: PairingConfig? = null,
    val endpoint: ResolvedEndpoint? = null,
    val serviceRunning: Boolean = false,
    val pendingCount: Int = 0,
)
