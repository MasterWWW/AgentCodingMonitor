package com.vibemonitor.bridge.service

import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationManagerCompat
import com.vibemonitor.bridge.BridgeRuntime
import com.vibemonitor.bridge.data.PairingStore
import com.vibemonitor.bridge.discovery.EndpointResolver
import com.vibemonitor.bridge.discovery.MdnsDiscovery
import com.vibemonitor.bridge.model.PendingAction
import com.vibemonitor.bridge.network.VibeApiClient
import com.vibemonitor.bridge.network.WatchStreamCallbacks
import com.vibemonitor.bridge.network.WatchStreamClient
import com.vibemonitor.bridge.watch.LogWatchRelay
import com.vibemonitor.bridge.watch.VivoWatchRelay
import com.vibemonitor.bridge.watch.WatchMessageParser
import com.vibemonitor.bridge.watch.WatchRelay
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking

/**
 * 前台服务：mDNS 发现 → SSE → 转发手表 / 手机通知兜底。
 */
class BridgeService : Service() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private lateinit var pairingStore: PairingStore
    private lateinit var endpointResolver: EndpointResolver
    private lateinit var streamClient: WatchStreamClient
    private lateinit var watchRelay: WatchRelay

    override fun onCreate() {
        super.onCreate()
        NotificationHelper.ensureChannels(this)
        pairingStore = PairingStore(this)
        val api = VibeApiClient()
        endpointResolver = EndpointResolver(MdnsDiscovery(this), api)
        streamClient = WatchStreamClient()
        watchRelay = selectWatchRelay()
        watchRelay.start { json -> handleWatchResponse(json) }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                stopSelf()
                return START_NOT_STICKY
            }
        }
        startForeground(
            NotificationHelper.NOTIFICATION_SERVICE_ID,
            NotificationHelper.serviceNotification(this, "正在启动…"),
        )
        BridgeRuntime.setServiceRunning(true)
        scope.launch { runBridgeLoop() }
        return START_STICKY
    }

    override fun onDestroy() {
        streamClient.disconnect()
        watchRelay.stop()
        BridgeRuntime.setServiceRunning(false)
        scope.cancel()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun selectWatchRelay(): WatchRelay {
        val vivo = VivoWatchRelay()
        return if (vivo.isAvailable()) vivo else LogWatchRelay()
    }

    private suspend fun runBridgeLoop() {
        while (true) {
            val pairing = pairingStore.pairingFlow.first()
            if (pairing == null) {
                BridgeRuntime.updatePhase("Idle", "请先扫码配对")
                updateServiceNotification("未配对")
                delay(3_000)
                continue
            }
            BridgeRuntime.updatePairing(pairing)
            BridgeRuntime.updatePhase("Discovering", "正在发现开发机…")
            updateServiceNotification("正在发现开发机…")

            val endpoint = endpointResolver.resolve(pairing)
            if (endpoint == null) {
                BridgeRuntime.updateEndpoint(null)
                BridgeRuntime.updatePhase("Error", "找不到开发机，请确认同一 WiFi 且已启用手表伴侣")
                updateServiceNotification("发现失败")
                delay(5_000)
                continue
            }

            BridgeRuntime.updateEndpoint(endpoint)
            BridgeRuntime.updatePhase("Connecting", "已连接 ${endpoint.host}")
            connectStream(endpoint, pairing.token)
            delay(3_000)
        }
    }

    private suspend fun connectStream(endpoint: com.vibemonitor.bridge.model.ResolvedEndpoint, token: String) {
        var connected = false
        val latch = kotlinx.coroutines.CompletableDeferred<Unit>()

        streamClient.connect(
            endpoint,
            token,
            object : WatchStreamCallbacks {
                override fun onConnected() {
                    connected = true
                    BridgeRuntime.updatePhase("Connected", "已同步 ${endpoint.host}")
                    updateServiceNotification("已连接 ${endpoint.host}")
                }

                override fun onDisconnected() {
                    if (connected) latch.complete(Unit)
                }

                override fun onActionCreated(action: PendingAction) {
                    onNewAction(action)
                }

                override fun onActionCancelled(actionId: String) {
                    BridgeRuntime.decrementPending()
                }

                override fun onError(message: String) {
                    BridgeRuntime.updatePhase("Error", message)
                }
            },
        )

        latch.await()
        streamClient.disconnect()
    }

    private fun onNewAction(action: PendingAction) {
        BridgeRuntime.incrementPending()
        val envelope = BridgeRuntime.api.buildActionPromptEnvelope(action)
        watchRelay.sendPrompt(envelope)
        NotificationManagerCompat.from(this).notify(
            action.id.hashCode(),
            NotificationHelper.actionNotification(this, action),
        )
    }

    private fun handleWatchResponse(json: String) {
        scope.launch {
            runCatching {
                val parsed = WatchMessageParser.parseResponse(json)
                if (parsed != null) {
                    BridgeRuntime.respond(parsed.actionId, parsed.choice, parsed.from)
                    BridgeRuntime.decrementPending()
                    return@launch
                }
                val root = org.json.JSONObject(json)
                val data = root.optJSONObject("data") ?: root
                val actionId = data.optString("action_id", data.optString("actionId"))
                val choice = data.optString("choice")
                val from = data.optString("from", "watch")
                if (actionId.isNotBlank() && choice.isNotBlank()) {
                    BridgeRuntime.respond(actionId, choice, from)
                    BridgeRuntime.decrementPending()
                }
            }.onFailure { e ->
                Log.w(TAG, "watch response failed", e)
            }
        }
    }

    private fun updateServiceNotification(text: String) {
        val nm = NotificationManagerCompat.from(this)
        nm.notify(
            NotificationHelper.NOTIFICATION_SERVICE_ID,
            NotificationHelper.serviceNotification(this, text),
        )
    }

    companion object {
        private const val TAG = "BridgeService"
        const val ACTION_STOP = "com.vibemonitor.bridge.STOP"

        fun start(context: Context) {
            val intent = Intent(context, BridgeService::class.java)
            context.startForegroundService(intent)
        }

        fun stop(context: Context) {
            val intent = Intent(context, BridgeService::class.java).apply { action = ACTION_STOP }
            context.startService(intent)
        }

        fun respondFromNotification(context: Context, actionId: String, choice: String) {
            runBlocking {
                val ok = BridgeRuntime.respond(actionId, choice, "phone")
                if (ok) BridgeRuntime.decrementPending()
            }
            NotificationManagerCompat.from(context).cancel(actionId.hashCode())
        }
    }
}
