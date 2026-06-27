package com.vibemonitor.bridge.discovery

import android.util.Log
import com.vibemonitor.bridge.model.PairingConfig
import com.vibemonitor.bridge.model.ResolvedEndpoint
import com.vibemonitor.bridge.network.VibeApiClient
import kotlinx.coroutines.withTimeoutOrNull

private const val TAG = "EndpointResolver"

/**
 * 按设计文档顺序解析开发机地址：mDNS → host_fallback → 失败。
 */
class EndpointResolver(
    private val mdns: MdnsDiscovery,
    private val api: VibeApiClient,
) {
    suspend fun resolve(pairing: PairingConfig): ResolvedEndpoint? {
        val mdnsResult = withTimeoutOrNull(8_000) {
            mdns.resolve(pairing)
        }
        if (mdnsResult != null && api.healthCheck(mdnsResult, pairing.token)) {
            Log.i(TAG, "resolved via mDNS: ${mdnsResult.baseUrl}")
            return mdnsResult
        }

        val fallbackHost = pairing.hostFallback?.trim().orEmpty()
        if (fallbackHost.isNotEmpty()) {
            val fallback = ResolvedEndpoint(
                host = fallbackHost,
                port = pairing.port,
                baseUrl = "http://$fallbackHost:${pairing.port}",
            )
            if (api.healthCheck(fallback, pairing.token)) {
                Log.i(TAG, "resolved via host_fallback: ${fallback.baseUrl}")
                return fallback
            }
        }

        Log.w(TAG, "could not resolve endpoint for device ${pairing.deviceId}")
        return null
    }
}
