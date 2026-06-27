package com.vibemonitor.bridge.network

import com.vibemonitor.bridge.model.ActionResponseBody
import com.vibemonitor.bridge.model.Envelope
import com.vibemonitor.bridge.model.PendingAction
import com.vibemonitor.bridge.model.ResolvedEndpoint
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.util.concurrent.TimeUnit

class VibeApiClient(
    private val client: OkHttpClient = OkHttpClient.Builder()
        .connectTimeout(5, TimeUnit.SECONDS)
        .readTimeout(15, TimeUnit.SECONDS)
        .build(),
    private val json: Json = Json { ignoreUnknownKeys = true },
) {
    fun healthCheck(endpoint: ResolvedEndpoint, token: String): Boolean {
        val request = Request.Builder()
            .url("${endpoint.baseUrl}/api/status?token=$token")
            .get()
            .build()
        return runCatching {
            client.newCall(request).execute().use { it.isSuccessful }
        }.getOrDefault(false)
    }

    fun respond(
        endpoint: ResolvedEndpoint,
        token: String,
        actionId: String,
        choice: String,
        from: String,
    ): Boolean {
        val body = json.encodeToString(
            ActionResponseBody(actionId = actionId, choice = choice, from = from),
        )
        val request = Request.Builder()
            .url("${endpoint.baseUrl}/api/actions/$actionId/respond?token=$token")
            .post(body.toRequestBody("application/json".toMediaType()))
            .build()
        return runCatching {
            client.newCall(request).execute().use { it.isSuccessful }
        }.getOrDefault(false)
    }

    fun buildActionPromptEnvelope(action: PendingAction): String {
        val envelope = Envelope(
            type = "action_prompt",
            id = action.id,
            ts = System.currentTimeMillis() / 1000,
            data = action,
        )
        return json.encodeToString(envelope)
    }

    fun buildActionResponseEnvelope(
        actionId: String,
        choice: String,
        from: String,
    ): String {
        val envelope = Envelope(
            type = "action_response",
            id = actionId,
            ts = System.currentTimeMillis() / 1000,
            data = ActionResponseBody(actionId = actionId, choice = choice, from = from),
        )
        return json.encodeToString(envelope)
    }
}
