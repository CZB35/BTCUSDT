package com.eventsignal.app.service

import android.app.Service
import android.content.Intent
import android.os.IBinder
import com.eventsignal.app.automation.AutomationStateStore
import com.eventsignal.app.data.EventSignalRepository
import com.eventsignal.app.data.SignalJsonParser
import com.eventsignal.app.domain.SignalDeliveryAckRequest
import com.eventsignal.app.domain.SignalSnapshot
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
import java.time.Instant
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
        stateStore.saveConnectionState("stopped")
        startForeground(
            1002,
            runtimeNotification(this, "Automation active", "Monitoring authorized action state"),
        )
        AutomationRuntime.running = true
        startSignalSocket()
        startRuntimeSummaryLoop()
        startPendingOrderReportLoop()
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
            stateStore.saveConnectionState("stopped")
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
                    stateStore.saveConnectionState("request_failed")
                    stateStore.saveLastStatus("ws_request_failed:${requestResult.exceptionOrNull()?.message}")
                    delay(2_000)
                    continue
                }
                stateStore.saveConnectionState("connecting")
                stateStore.saveLastStatus("ws_connecting")
                val closed = AtomicBoolean(false)
                webSocket = wsClient.newWebSocket(
                    request,
                    object : WebSocketListener() {
                        override fun onOpen(webSocket: WebSocket, response: Response) {
                            stateStore.saveConnectionState("connected")
                            stateStore.saveLastStatus("ws_connected")
                        }

                        override fun onMessage(webSocket: WebSocket, text: String) {
                            handleSocketMessage(text)
                        }

                        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                            closed.set(true)
                            stateStore.saveConnectionState("failed")
                            stateStore.saveLastStatus("ws_failed:${t.javaClass.simpleName}")
                        }

                        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                            closed.set(true)
                            stateStore.saveConnectionState("closed:$code")
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

    private var summaryLoopStarted = false

    private fun startRuntimeSummaryLoop() {
        if (summaryLoopStarted) return
        summaryLoopStarted = true
        scope.launch {
            val repository = EventSignalRepository(applicationContext)
            while (isActive) {
                repository.loadAppRuntimeSummary()
                    .onFailure { stateStore.saveLastStatus("runtime_summary_failed:${it.javaClass.simpleName}") }
                delay(5_000)
            }
        }
    }

    private var pendingOrderLoopStarted = false

    private fun startPendingOrderReportLoop() {
        if (pendingOrderLoopStarted) return
        pendingOrderLoopStarted = true
        scope.launch {
            val repository = EventSignalRepository(applicationContext)
            while (isActive) {
                val pendingOrders = stateStore.loadPendingOrders()
                for (pending in pendingOrders) {
                    repository.submitEventContractOrder(pending.order)
                        .onSuccess {
                            stateStore.markExecuted(pending.signalId)
                            stateStore.removePendingOrder(pending.clientOrderId)
                            stateStore.saveLastStatus("pending_order_reported:${pending.order.stakeUsdt}")
                        }
                        .onFailure {
                            stateStore.savePendingOrder(pending.nextAttempt())
                            stateStore.saveLastStatus("pending_order_retry:${pending.attempts + 1}")
                        }
                }
                delay(2_000)
            }
        }
    }

    private fun handleSocketMessage(text: String) {
        if (text.contains("\"connection.closed\"")) {
            if (text.contains("AUTH_SESSION_REVOKED") || text.contains("DEVICE_SESSION_REPLACED")) {
                EventSignalRepository(applicationContext).logout()
                stateStore.saveConnectionState("auth_required")
                stateStore.saveLastStatus("ws_session_revoked")
                stopSelf()
                return
            }
            stateStore.saveLastStatus("ws_closed_by_server")
            return
        }
        if (!text.contains("\"signal.created\"")) return
        val signal = runCatching { SignalJsonParser.parse(text) }
            .onFailure { stateStore.saveLastStatus("ws_signal_parse_failed:${it.javaClass.simpleName}") }
            .getOrNull() ?: return
        val signalId = signal.id ?: return
        ackSignalReceived(signal)
        if (!handledSignalIds.add(signalId)) {
            stateStore.saveLastStatus("ws_signal_duplicate_ignored:$signalId")
            return
        }
        if (signal.direction.uppercase() !in setOf("UP", "DOWN")) {
            stateStore.saveLastStatus("ws_signal_ignored")
            return
        }
        if (!signal.tradeAuthorized) {
            when (signal.authorizationStatus) {
                "fee_reserve_insufficient" -> {
                    AutomationRuntime.stop()
                    stateStore.saveLastStatus("ws_signal_not_authorized_fee_reserve_insufficient")
                }
                "mismatch",
                "observed_balance_missing",
                "exchange_balance_missing",
                "exchange_balance_stale",
                "unknown",
                -> {
                    AutomationRuntime.stop()
                    stateStore.saveLastStatus("ws_signal_not_authorized_account_gate")
                }
                else -> stateStore.saveLastStatus("ws_signal_not_authorized")
            }
            return
        }
        stateStore.saveSignal(signal)
        stateStore.saveLastStatus("ws_signal_execute:$signalId")
        AutomationRuntime.requestExecution()
    }

    private fun ackSignalReceived(signal: SignalSnapshot) {
        val deliveryId = signal.deliveryId ?: return
        val signalId = signal.id ?: return
        scope.launch {
            val repository = EventSignalRepository(applicationContext)
            val debug = stateStore.loadDebugState()
            val result = repository.ackSignalDelivery(
                SignalDeliveryAckRequest(
                    deliveryId = deliveryId,
                    signalId = signalId,
                    reservationId = signal.reservationId,
                    receivedAt = Instant.now().toString(),
                    appVersion = packageManager.getPackageInfo(packageName, 0).versionName,
                    pageState = mapOf(
                        "screen_name" to debug.screenName,
                        "confidence" to debug.confidence.toString(),
                        "package_name" to debug.packageName,
                        "amount_field_found" to debug.amountFieldFound.toString(),
                        "up_button_found" to debug.upButtonFound.toString(),
                        "down_button_found" to debug.downButtonFound.toString(),
                        "safe_to_act" to debug.safeToAct.toString(),
                        "observed_balance_usdt" to (debug.observedBalanceUsdt ?: "-"),
                    ),
                ),
            )
            result.onFailure {
                stateStore.saveLastStatus("ws_ack_failed:${it.javaClass.simpleName}")
            }
        }
    }
}
