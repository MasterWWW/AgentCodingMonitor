package com.vibemonitor.bridge.discovery

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.util.Log
import com.vibemonitor.bridge.model.PairingConfig
import com.vibemonitor.bridge.model.ResolvedEndpoint
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.resume

private const val TAG = "MdnsDiscovery"
private const val SERVICE_TYPE = "_vibe-monitor._tcp."

/**
 * 通过 NsdManager 解析 PC 端 mDNS 服务（`vibe-monitor-{device_id}`）。
 */
class MdnsDiscovery(context: Context) {

    private val nsdManager: NsdManager =
        context.getSystemService(Context.NSD_SERVICE) as NsdManager

    suspend fun resolve(pairing: PairingConfig): ResolvedEndpoint? =
        suspendCancellableCoroutine { cont ->
            lateinit var discoveryListener: NsdManager.DiscoveryListener
            discoveryListener = object : NsdManager.DiscoveryListener {
                override fun onStartDiscoveryFailed(type: String?, code: Int) {
                    Log.w(TAG, "startDiscoveryFailed $code")
                    if (cont.isActive) cont.resume(null)
                }

                override fun onStopDiscoveryFailed(type: String?, code: Int) {
                    Log.w(TAG, "stopDiscoveryFailed $code")
                }

                override fun onDiscoveryStarted(type: String?) {
                    Log.d(TAG, "discovery started")
                }

                override fun onDiscoveryStopped(type: String?) {
                    Log.d(TAG, "discovery stopped")
                }

                override fun onServiceFound(info: NsdServiceInfo) {
                    if (!info.serviceName.contains(pairing.deviceId)) return
                    nsdManager.resolveService(info, object : NsdManager.ResolveListener {
                        override fun onResolveFailed(serviceInfo: NsdServiceInfo?, code: Int) {
                            Log.w(TAG, "resolveFailed $code")
                        }

                        override fun onServiceResolved(resolved: NsdServiceInfo) {
                            val host = resolved.host?.hostAddress
                            val port = resolved.port
                            if (host != null && port > 0 && cont.isActive) {
                                runCatching { nsdManager.stopServiceDiscovery(discoveryListener) }
                                cont.resume(
                                    ResolvedEndpoint(
                                        host = host,
                                        port = port,
                                        baseUrl = "http://$host:$port",
                                    ),
                                )
                            }
                        }
                    })
                }

                override fun onServiceLost(info: NsdServiceInfo) {
                    Log.d(TAG, "service lost ${info.serviceName}")
                }
            }

            cont.invokeOnCancellation {
                runCatching { nsdManager.stopServiceDiscovery(discoveryListener) }
            }

            nsdManager.discoverServices(
                SERVICE_TYPE,
                NsdManager.PROTOCOL_DNS_SD,
                discoveryListener,
            )
        }
}
