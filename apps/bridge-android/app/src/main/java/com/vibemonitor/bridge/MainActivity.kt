package com.vibemonitor.bridge

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.ContextCompat
import com.google.zxing.integration.android.IntentIntegrator
import com.vibemonitor.bridge.data.PairingStore
import com.vibemonitor.bridge.ui.BridgeViewModel
import com.vibemonitor.bridge.ui.MainScreen

class MainActivity : ComponentActivity() {

    private val pairingStore by lazy { PairingStore(this) }
    private val viewModel: BridgeViewModel by viewModels {
        BridgeViewModel.Factory(pairingStore)
    }

    private var errorMessage by mutableStateOf<String?>(null)

    private val permissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) { /* no-op */ }

    private val scanLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        val scan = IntentIntegrator.parseActivityResult(result.resultCode, result.data)
        if (scan?.contents != null) {
            viewModel.savePairingFromJson(scan.contents) { msg ->
                errorMessage = msg
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestRuntimePermissions()

        setContent {
            val state by viewModel.uiState.collectAsState()
            MaterialTheme {
                Surface {
                    MainScreen(
                        state = state,
                        onScanQr = { launchQrScan() },
                        onPasteJson = { raw ->
                            viewModel.savePairingFromJson(raw) { msg ->
                                errorMessage = msg
                            }
                        },
                        onStart = { viewModel.startBridge(this) },
                        onStop = { viewModel.stopBridge(this) },
                        onClear = { viewModel.clearPairing(this) },
                        errorMessage = errorMessage,
                    )
                }
            }
        }
    }

    private fun requestRuntimePermissions() {
        val needed = mutableListOf<String>()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            needed.add(Manifest.permission.POST_NOTIFICATIONS)
        }
        if (ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            needed.add(Manifest.permission.CAMERA)
        }
        if (needed.isNotEmpty()) {
            permissionLauncher.launch(needed.toTypedArray())
        }
    }

    private fun launchQrScan() {
        val integrator = IntentIntegrator(this)
            .setDesiredBarcodeFormats(IntentIntegrator.QR_CODE)
            .setPrompt("扫描 Vibe Monitor 托盘中的手表配对二维码")
            .setBeepEnabled(false)
        scanLauncher.launch(integrator.createScanIntent())
    }
}
