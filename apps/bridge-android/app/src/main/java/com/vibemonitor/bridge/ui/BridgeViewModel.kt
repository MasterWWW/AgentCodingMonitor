package com.vibemonitor.bridge.ui

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import com.vibemonitor.bridge.BridgeRuntime
import com.vibemonitor.bridge.data.PairingStore
import com.vibemonitor.bridge.model.BridgeUiState
import com.vibemonitor.bridge.model.ConnectionPhase
import com.vibemonitor.bridge.service.BridgeService
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch

class BridgeViewModel(
    private val pairingStore: PairingStore,
) : ViewModel() {

    private val _uiState = MutableStateFlow(BridgeUiState())
    val uiState: StateFlow<BridgeUiState> = _uiState.asStateFlow()

    init {
        viewModelScope.launch {
            combine(pairingStore.pairingFlow, BridgeRuntime.state) { pairing, runtime ->
                BridgeUiState(
                    phase = mapPhase(runtime.phase),
                    message = runtime.message,
                    pairing = pairing ?: runtime.pairing,
                    endpoint = runtime.endpoint,
                    serviceRunning = runtime.serviceRunning,
                    pendingCount = runtime.pendingCount,
                )
            }.collect { _uiState.value = it }
        }
    }

    fun savePairingFromJson(raw: String, onError: (String) -> Unit) {
        viewModelScope.launch {
            runCatching {
                val config = pairingStore.parsePairingJson(raw)
                pairingStore.save(config)
                BridgeRuntime.updatePairing(config)
            }.onFailure { onError(it.message ?: "配对 JSON 无效") }
        }
    }

    fun clearPairing(context: Context) {
        viewModelScope.launch {
            BridgeService.stop(context)
            pairingStore.clear()
            BridgeRuntime.updatePairing(null)
            BridgeRuntime.updateEndpoint(null)
            BridgeRuntime.updatePhase("Idle", "未配对")
        }
    }

    fun startBridge(context: Context) {
        BridgeService.start(context)
    }

    fun stopBridge(context: Context) {
        BridgeService.stop(context)
    }

    private fun mapPhase(name: String): ConnectionPhase =
        runCatching { ConnectionPhase.valueOf(name) }.getOrDefault(ConnectionPhase.Idle)

    class Factory(private val pairingStore: PairingStore) : ViewModelProvider.Factory {
        @Suppress("UNCHECKED_CAST")
        override fun <T : ViewModel> create(modelClass: Class<T>): T {
            return BridgeViewModel(pairingStore) as T
        }
    }
}
