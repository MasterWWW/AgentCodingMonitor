package com.vibemonitor.bridge.network

import android.util.Log
import kotlinx.serialization.json.decodeFromJsonElement
import com.vibemonitor.bridge.model.CancelledPayload
import com.vibemonitor.bridge.model.PendingAction
import com.vibemonitor.bridge.model.ResolvedEndpoint
import com.vibemonitor.bridge.model.WatchStreamEvent
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.sse.EventSource
import okhttp3.sse.EventSourceListener
import okhttp3.sse.EventSources
import java.util.concurrent.TimeUnit

private const val TAG = "WatchStream"

interface WatchStreamCallbacks {
    fun onConnected()
    fun onDisconnected()
    fun onActionCreated(action: PendingAction)
    fun onActionCancelled(actionId: String)
    fun onError(message: String)
}

/**
 * SSE 客户端：`GET /api/watch/stream?token=...`
 */
class WatchStreamClient(
    private val json: Json = Json { ignoreUnknownKeys = true },
    private val client: OkHttpClient = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.SECONDS)
        .build(),
) {
    private var eventSource: EventSource? = null

    fun connect(
        endpoint: ResolvedEndpoint,
        token: String,
        callbacks: WatchStreamCallbacks,
    ) {
        disconnect()
        val request = Request.Builder()
            .url("${endpoint.baseUrl}/api/watch/stream?token=$token")
            .header("Accept", "text/event-stream")
            .get()
            .build()

        eventSource = EventSources.createFactory(client).newEventSource(
            request,
            object : EventSourceListener() {
                override fun onOpen(eventSource: EventSource, response: okhttp3.Response) {
                    Log.i(TAG, "SSE open")
                    callbacks.onConnected()
                }

                override fun onEvent(
                    eventSource: EventSource,
                    id: String?,
                    type: String?,
                    data: String,
                ) {
                    handleEvent(type, data, callbacks)
                }

                override fun onClosed(eventSource: EventSource) {
                    Log.i(TAG, "SSE closed")
                    callbacks.onDisconnected()
                }

                override fun onFailure(
                    eventSource: EventSource,
                    t: Throwable?,
                    response: okhttp3.Response?,
                ) {
                    Log.w(TAG, "SSE failure", t)
                    callbacks.onDisconnected()
                    callbacks.onError(t?.message ?: "连接断开")
                }
            },
        )
    }

    fun disconnect() {
        eventSource?.cancel()
        eventSource = null
    }

    private fun handleEvent(type: String?, data: String, callbacks: WatchStreamCallbacks) {
        if (type.isNullOrBlank()) return
        runCatching {
            val event = json.decodeFromString<WatchStreamEvent>(data)
            when (event.event) {
                "action_created" -> {
                    val action = json.decodeFromJsonElement<PendingAction>(event.data)
                    callbacks.onActionCreated(action)
                }
                "action_cancelled" -> {
                    val payload = json.decodeFromJsonElement<CancelledPayload>(event.data)
                    callbacks.onActionCancelled(payload.id)
                }
            }
        }.onFailure { e ->
            Log.w(TAG, "parse event failed: $type", e)
        }
    }
}
