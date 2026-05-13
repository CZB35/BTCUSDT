package com.eventsignal.app.service

import android.app.Service
import android.content.Intent
import android.os.IBinder
import com.eventsignal.app.automation.AutomationStateStore
import com.eventsignal.app.data.EventSignalRepository
import com.eventsignal.app.data.SignalJsonParser
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

class AutomationForegroundService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val wsClient = OkHttpClient.Builder()
        .connectTimeout(2, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.SECONDS)
        .pingInterval(15, TimeUnit.SECONDS)
        .build()
    private lateinit var stateStore: AutomationStateStore
    private var webSocket: WebSocket? = null
    private val handledSignalIds = ConcurrentHashMap.newKeySet<String>()

    override fun onCreate() {
        super.onCreate()
        stateStore = AutomationStateStore(applicationContext)
        startForeground(
            1002,
            runtimeNotification(this, "Automation active", "Monitoring authorized action state"),
        )
        AutomationRuntime.running = true
        startSignalSocket()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        AutomationRuntime.running = true
        startSignalSocket()
        return START_STICKY
    }

    override fun onDestroy() {
        AutomationRuntime.running = false
        webSocket?.cancel()
        webSocket = null
        if (::stateStore.isInitialized) {
            stateStore.clearRecognitionState("\u81ea\u52a8\u5316\u670d\u52a1\u5df2\u505c\u6b62")
        }
        scope.cancel()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private var socketLoopStarted = false

    private fun startSignalSocket() {
        if (socketLoopStarted) return
        socketLoopStarted = true
        scope.launch {
            val repository = EventSignalRepository(applicationContext)
            while (isActive) {
                repository.loadAutomationSettings()
                val requestResult = repository.websocketRequest()
                val request = requestResult.getOrNull()
                if (request == null) {
                    stateStore.saveLastStatus("ws_request_failed:${requestResult.exceptionOrNull()?.message}")
                    delay(2_000)
                    continue
                }
                stateStore.saveLastStatus("ws_connecting")
                val closed = AtomicBoolean(false)
                webSocket = wsClient.newWebSocket(
                    request,
                    object : WebSocketListener() {
                        override fun onOpen(webSocket: WebSocket, response: Response) {
                            stateStore.saveLastStatus("ws_connected")
                        }

                        override fun onMessage(webSocket: WebSocket, text: String) {
                            handleSocketMessage(text)
                        }

                        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                            closed.set(true)
                            stateStore.saveLastStatus("ws_failed:${t.javaClass.simpleName}")
                        }

                        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                            closed.set(true)
                            stateStore.saveLastStatus("ws_closed:$code")
                        }
                    },
                )
                while (isActive && AutomationRuntime.running && !closed.get()) {
                    delay(1_000)
                }
                webSocket?.cancel()
                webSocket = null
                if (isActive && AutomationRuntime.running) {
                    delay(1_000)
                }
            }
        }
    }

    private fun handleSocketMessage(text: String) {
        if (!text.contains("\"signal.created\"")) return
        val signal = runCatching { SignalJsonParser.parse(text) }
            .onFailure { stateStore.saveLastStatus("ws_signal_parse_failed:${it.javaClass.simpleName}") }
            .getOrNull() ?: return
        val signalId = signal.id ?: return
        if (!handledSignalIds.add(signalId)) {
            stateStore.saveLastStatus("ws_signal_duplicate_ignored:$signalId")
            return
        }
        if (signal.direction.uppercase() !in setOf("UP", "DOWN")) {
            stateStore.saveLastStatus("ws_signal_ignored_direction:${signal.direction}")
            return
        }
        if (!signal.tradeAuthorized) {
            stateStore.saveLastStatus("ws_signal_not_authorized:${signal.authorizationStatus ?: "-"}")
            return
        }
        stateStore.saveSignal(signal)
        stateStore.saveLastStatus("ws_signal_execute:$signalId")
        AutomationRuntime.requestExecution()
    }
}
