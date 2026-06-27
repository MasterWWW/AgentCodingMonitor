package com.vibemonitor.bridge.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.vibemonitor.bridge.model.BridgeUiState
import com.vibemonitor.bridge.model.ConnectionPhase

@Composable
fun MainScreen(
    state: BridgeUiState,
    onScanQr: () -> Unit,
    onPasteJson: (String) -> Unit,
    onStart: () -> Unit,
    onStop: () -> Unit,
    onClear: () -> Unit,
    errorMessage: String?,
) {
    var jsonInput by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Vibe Bridge", style = MaterialTheme.typography.headlineMedium)
        Text("将 Agent 待确认转发到 vivo Watch", style = MaterialTheme.typography.bodyMedium)

        Card(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("状态：${phaseLabel(state.phase)}", style = MaterialTheme.typography.titleMedium)
                Text(state.message)
                state.endpoint?.let { Text("开发机：${it.baseUrl}") }
                state.pairing?.let { Text("设备 ID：${it.deviceId}") }
                if (state.pendingCount > 0) {
                    Text("待确认：${state.pendingCount}")
                }
            }
        }

        Button(onClick = onScanQr, modifier = Modifier.fillMaxWidth()) {
            Text("扫描桌面配对二维码")
        }

        OutlinedTextField(
            value = jsonInput,
            onValueChange = { jsonInput = it },
            modifier = Modifier.fillMaxWidth(),
            label = { Text("或粘贴配对 JSON") },
            minLines = 3,
        )
        Button(
            onClick = { onPasteJson(jsonInput) },
            modifier = Modifier.fillMaxWidth(),
            enabled = jsonInput.isNotBlank(),
        ) {
            Text("保存配对")
        }

        if (state.pairing != null) {
            if (state.serviceRunning) {
                Button(onClick = onStop, modifier = Modifier.fillMaxWidth()) {
                    Text("停止桥接服务")
                }
            } else {
                Button(onClick = onStart, modifier = Modifier.fillMaxWidth()) {
                    Text("启动桥接服务")
                }
            }
            Button(onClick = onClear, modifier = Modifier.fillMaxWidth()) {
                Text("清除配对")
            }
        }

        errorMessage?.let {
            Spacer(modifier = Modifier.height(4.dp))
            Text(it, color = MaterialTheme.colorScheme.error)
        }

        Spacer(modifier = Modifier.height(8.dp))
        Text(
            "提示：开发机需启用「手表伴侣」；手机与电脑同一 WiFi。无手表 SDK 时，确认通知会显示在本机。",
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

private fun phaseLabel(phase: ConnectionPhase): String = when (phase) {
    ConnectionPhase.Idle -> "空闲"
    ConnectionPhase.Discovering -> "发现中"
    ConnectionPhase.Connecting -> "连接中"
    ConnectionPhase.Connected -> "已连接"
    ConnectionPhase.Error -> "异常"
}
