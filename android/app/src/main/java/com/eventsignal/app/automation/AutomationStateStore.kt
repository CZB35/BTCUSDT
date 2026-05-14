package com.eventsignal.app.automation

import android.content.Context
import com.eventsignal.app.domain.AutomationSettings
import com.eventsignal.app.domain.AppRuntimeSummary
import com.eventsignal.app.domain.SignalSnapshot
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

class AutomationStateStore(context: Context) {
    private val prefs = context.applicationContext.getSharedPreferences("automation_state", Context.MODE_PRIVATE)
    private val json = Json {
        ignoreUnknownKeys = true
    }

    companion object {
        private const val MIN_DEBUG_DISK_WRITE_MS = 1_000L
        private const val MIN_STATUS_DISK_WRITE_MS = 1_000L

        @Volatile
        private var cachedSignal: SignalSnapshot? = null

        @Volatile
        private var cachedSettings: AutomationSettings? = null

        @Volatile
        private var cachedDebugState: AutomationDebugState? = null

        @Volatile
        private var cachedLastStatus: String? = null

        @Volatile
        private var cachedConnectionState: String? = null

        @Volatile
        private var cachedRuntimeSummary: AppRuntimeSummary? = null

        @Volatile
        private var lastDebugDiskWriteAtMs: Long = 0

        @Volatile
        private var lastStatusDiskWriteAtMs: Long = 0
    }

    fun saveSignal(signal: SignalSnapshot) {
        cachedSignal = signal
        prefs.edit()
            .putString("signal", json.encodeToString(signal))
            .putLong("signal_cached_at_ms", System.currentTimeMillis())
            .apply()
    }

    fun loadSignal(): SignalSnapshot? {
        cachedSignal?.let { return it }
        val raw = prefs.getString("signal", null) ?: return null
        return runCatching { json.decodeFromString<SignalSnapshot>(raw) }.getOrNull()
            ?.also { cachedSignal = it }
    }

    fun saveSettings(settings: AutomationSettings) {
        cachedSettings = settings
        prefs.edit()
            .putString("settings", json.encodeToString(settings))
            .putLong("settings_cached_at_ms", System.currentTimeMillis())
            .apply()
    }

    fun loadSettings(): AutomationSettings =
        cachedSettings ?: prefs.getString("settings", null)
            ?.let { raw -> runCatching { json.decodeFromString<AutomationSettings>(raw) }.getOrNull() }
            ?.also { cachedSettings = it }
            ?: AutomationSettings.default()

    fun hasExecuted(signalId: String): Boolean =
        prefs.getStringSet("executed_signal_ids", emptySet()).orEmpty().contains(signalId)

    fun markExecuted(signalId: String) {
        val next = prefs.getStringSet("executed_signal_ids", emptySet()).orEmpty().toMutableSet()
        next.add(signalId)
        prefs.edit().putStringSet("executed_signal_ids", next.toList().takeLast(100).toSet()).apply()
    }

    fun saveLastStatus(status: String) {
        val now = System.currentTimeMillis()
        if (cachedLastStatus == status && now - lastStatusDiskWriteAtMs < MIN_STATUS_DISK_WRITE_MS) {
            return
        }
        cachedLastStatus = status
        lastStatusDiskWriteAtMs = now
        prefs.edit()
            .putString("last_status", status)
            .putLong("last_status_at_ms", now)
            .apply()
    }

    fun loadLastStatus(): String =
        cachedLastStatus ?: (prefs.getString("last_status", "-") ?: "-").also { cachedLastStatus = it }

    fun saveConnectionState(state: String) {
        cachedConnectionState = state
        prefs.edit()
            .putString("connection_state", state)
            .putLong("connection_state_at_ms", System.currentTimeMillis())
            .apply()
    }

    fun loadConnectionState(): String =
        cachedConnectionState ?: (prefs.getString("connection_state", "unknown") ?: "unknown").also {
            cachedConnectionState = it
        }

    fun saveRuntimeSummary(summary: AppRuntimeSummary) {
        cachedRuntimeSummary = summary
        prefs.edit()
            .putString("runtime_summary", json.encodeToString(summary))
            .apply()
    }

    fun loadRuntimeSummary(): AppRuntimeSummary =
        cachedRuntimeSummary ?: prefs.getString("runtime_summary", null)
            ?.let { raw -> runCatching { json.decodeFromString<AppRuntimeSummary>(raw) }.getOrNull() }
            ?.also { cachedRuntimeSummary = it }
            ?: AppRuntimeSummary()

    fun saveDebugState(state: AutomationDebugState) {
        val now = System.currentTimeMillis()
        val next = state.copy(updatedAtMs = now)
        val previous = cachedDebugState
        cachedDebugState = next
        if (
            previous != null &&
            previous.sameDisplayState(next) &&
            now - lastDebugDiskWriteAtMs < MIN_DEBUG_DISK_WRITE_MS
        ) {
            return
        }
        lastDebugDiskWriteAtMs = now
        prefs.edit()
            .putString("debug_state", json.encodeToString(next))
            .apply()
    }

    fun clearRecognitionState(lastAction: String) {
        val next = AutomationDebugState(
            accessibilityConnected = true,
            lastEventAtMs = 0,
            lastAction = lastAction,
            updatedAtMs = System.currentTimeMillis(),
        )
        if (cachedDebugState?.sameDisplayState(next) == true) return
        cachedDebugState = next
        lastDebugDiskWriteAtMs = next.updatedAtMs
        prefs.edit()
            .putString(
                "debug_state",
                json.encodeToString(next),
            )
            .apply()
    }

    fun loadDebugState(): AutomationDebugState =
        cachedDebugState ?: prefs.getString("debug_state", null)
            ?.let { raw -> runCatching { json.decodeFromString<AutomationDebugState>(raw) }.getOrNull() }
            ?.also { cachedDebugState = it }
            ?: AutomationDebugState()

    private fun AutomationDebugState.sameDisplayState(other: AutomationDebugState): Boolean =
        copy(lastEventAtMs = 0, updatedAtMs = 0) == other.copy(lastEventAtMs = 0, updatedAtMs = 0)
}
