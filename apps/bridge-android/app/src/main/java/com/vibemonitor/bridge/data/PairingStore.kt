package com.vibemonitor.bridge.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.vibemonitor.bridge.model.PairingConfig
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

private val Context.pairingDataStore: DataStore<Preferences> by preferencesDataStore(name = "pairing")

class PairingStore(private val context: Context) {
    private val json = Json { ignoreUnknownKeys = true }

    private val keyConfig = stringPreferencesKey("pairing_json")

    val pairingFlow: Flow<PairingConfig?> = context.pairingDataStore.data.map { prefs ->
        prefs[keyConfig]?.let { raw ->
            runCatching { json.decodeFromString<PairingConfig>(raw) }.getOrNull()
        }
    }

    suspend fun save(config: PairingConfig) {
        context.pairingDataStore.edit { prefs ->
            prefs[keyConfig] = json.encodeToString(config)
        }
    }

    suspend fun clear() {
        context.pairingDataStore.edit { it.remove(keyConfig) }
    }

    fun parsePairingJson(raw: String): PairingConfig {
        return json.decodeFromString(raw.trim())
    }
}
