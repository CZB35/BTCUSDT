package com.eventsignal.app.service

import android.accessibilityservice.AccessibilityService
import android.view.accessibility.AccessibilityEvent
import com.eventsignal.app.automation.ActionRunner
import com.eventsignal.app.automation.AutomationDebugState
import com.eventsignal.app.automation.AutomationStateStore
import com.eventsignal.app.automation.BuiltInProfiles
import com.eventsignal.app.automation.RecognizedScreen
import com.eventsignal.app.automation.ScreenRecognitionService
import com.eventsignal.app.data.EventSignalRepository
import com.eventsignal.app.domain.AutomationPageStateRequest
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.time.Instant
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

class ScreenRecognitionAccessibilityService : AccessibilityService() {
    private lateinit var recognition: ScreenRecognitionService
    private lateinit var actionRunner: ActionRunner
    private lateinit var stateStore: AutomationStateStore
    private lateinit var repository: EventSignalRepository
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private val pageGateReportInFlight = AtomicBoolean(false)
    private val pageGateFailureCount = AtomicInteger(0)
    private val latestPageGatePayload = AtomicReference<AutomationPageStateRequest?>(null)
    private var handledCalibrationVersion = 0L
    private var handledExecutionVersion = 0L

    override fun onServiceConnected() {
        super.onServiceConnected()
        recognition = ScreenRecognitionService()
        stateStore = AutomationStateStore(applicationContext)
        repository = EventSignalRepository(applicationContext)
        actionRunner = ActionRunner(this, stateStore)
        stateStore.saveDebugState(
            AutomationDebugState(
                accessibilityConnected = true,
                lastAction = "\u65e0\u969c\u788d\u670d\u52a1\u5df2\u8fde\u63a5",
            ),
        )
        startServiceLoop()
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        // Long-running recognition is intentionally disabled. The app only scans
        // when the user requests page calibration or when an action is attempted.
    }

    private fun startServiceLoop() {
        scope.launch {
            while (isActive) {
                val version = AutomationRuntime.calibrationRequestVersion()
                if (version != handledCalibrationVersion) {
                    handledCalibrationVersion = version
                    calibrateCurrentPage()
                }
                val executionVersion = AutomationRuntime.executionRequestVersion()
                if (executionVersion != handledExecutionVersion) {
                    handledExecutionVersion = executionVersion
                    executeCurrentSignal()
                }
                refreshPageGate()
                delay(500)
            }
        }
    }

    private fun refreshPageGate() {
        if (!AutomationRuntime.running) return
        val settings = stateStore.loadSettings()
        if (!settings.automationEnabled || settings.executionMode != "automatic") return
        val root = rootInActiveWindow ?: run {
            stateStore.clearRecognitionState("\u9875\u9762\u95f8\u95e8\uff1a\u672a\u8bfb\u5230\u7a97\u53e3")
            return
        }
        val profile = BuiltInProfiles.byId(settings.targetAppProfile)
        val screen = recognition.recognize(root, null, profile)
        saveScreenState(screen, if (screen.safeToAct) "\u9875\u9762\u95f8\u95e8\u5df2\u5c31\u7eea" else "\u9875\u9762\u95f8\u95e8\u672a\u5c31\u7eea")
        val payload = AutomationPageStateRequest(
            pageReady = screen.safeToAct,
            screenName = screen.screenName,
            confidence = "%.6f".format(screen.confidence),
            packageName = screen.packageName,
            amountFieldFound = screen.amountFieldFound,
            upButtonFound = screen.upButtonFound,
            downButtonFound = screen.downButtonFound,
            observedBalanceUsdt = screen.observedBalanceUsdt,
            observedAt = Instant.now().toString(),
        )
        latestPageGatePayload.set(payload)
        if (!pageGateReportInFlight.compareAndSet(false, true)) return
        scope.launch(Dispatchers.IO) {
            try {
                reportLatestPageGate()
            } finally {
                pageGateReportInFlight.set(false)
            }
        }
    }

    private suspend fun reportLatestPageGate() {
        repeat(3) { attempt ->
            val payload = latestPageGatePayload.get() ?: return
            val result = repository.submitAutomationPageState(payload)
            if (result.isSuccess) {
                pageGateFailureCount.set(0)
                return
            }
            if (attempt < 2) {
                delay(300)
                return@repeat
            }
            val failures = pageGateFailureCount.incrementAndGet()
            if (failures >= 3) {
                val error = result.exceptionOrNull()
                stateStore.saveLastStatus("page_gate_report_failed:${error?.javaClass?.simpleName ?: "Unknown"}")
            }
        }
    }

    private fun calibrateCurrentPage() {
        val root = rootInActiveWindow ?: run {
            stateStore.clearRecognitionState("\u8bc6\u522b\u5931\u8d25\uff1a\u672a\u8bfb\u5230\u5f53\u524d\u7a97\u53e3")
            stateStore.saveLastStatus("calibration_missing_active_window")
            return
        }
        val settings = stateStore.loadSettings()
        val profile = BuiltInProfiles.byId(settings.targetAppProfile)
        val screen = recognition.recognize(root, null, profile)
        saveScreenState(
            screen,
            if (screen.safeToAct) {
                "\u9875\u9762\u6821\u51c6\u901a\u8fc7"
            } else {
                "\u9875\u9762\u6821\u51c6\u672a\u901a\u8fc7"
            },
        )
        stateStore.saveLastStatus("calibrated:${screen.screenName}:${screen.confidence}")
    }

    private fun executeCurrentSignal() {
        if (!AutomationRuntime.running) return
        val settings = stateStore.loadSettings()
        if (!settings.automationEnabled) {
            stateStore.saveLastStatus("execution_skipped_automation_disabled")
            return
        }
        if (settings.executionMode != "automatic") {
            stateStore.saveLastStatus("execution_skipped_automation_mode_invalid")
            return
        }
        val signal = stateStore.loadSignal() ?: run {
            stateStore.saveLastStatus("execution_failed_missing_signal")
            return
        }
        val root = rootInActiveWindow ?: run {
            stateStore.clearRecognitionState("\u6267\u884c\u5931\u8d25\uff1a\u672a\u8bfb\u5230\u5f53\u524d\u7a97\u53e3")
            stateStore.saveLastStatus("execution_failed_missing_active_window")
            return
        }
        val profile = BuiltInProfiles.byId(settings.targetAppProfile)
        val screen = recognition.recognize(root, null, profile)
        saveScreenState(
            screen,
            if (screen.safeToAct) {
                "\u6536\u5230\u4fe1\u53f7\uff0c\u5f00\u59cb\u6267\u884c"
            } else {
                "\u6267\u884c\u5931\u8d25\uff1a\u5f53\u524d\u4e0d\u662f\u4e8b\u4ef6\u5408\u7ea6\u9875"
            },
        )
        if (!screen.safeToAct) {
            stateStore.saveLastStatus("execution_failed_screen_not_recognized:${screen.screenName}:${screen.confidence}")
            return
        }
        scope.launch(Dispatchers.IO) {
            actionRunner.maybeRun(screen, profile, settings, signal)
        }
    }

    private fun saveScreenState(screen: RecognizedScreen, action: String) {
        stateStore.saveDebugState(
            AutomationDebugState(
                accessibilityConnected = true,
                lastEventAtMs = System.currentTimeMillis(),
                packageName = screen.packageName,
                screenName = screen.screenName,
                confidence = screen.confidence,
                safeToAct = screen.safeToAct,
                pageKeywordHits = screen.pageKeywordHits,
                amountFieldFound = screen.amountFieldFound,
                upButtonFound = screen.upButtonFound,
                downButtonFound = screen.downButtonFound,
                observedBalanceUsdt = screen.observedBalanceUsdt,
                lastAction = action,
                visibleText = screen.visibleText.take(12),
            ),
        )
    }

    override fun onInterrupt() {
        AutomationRuntime.stop()
        if (::stateStore.isInitialized) {
            stateStore.clearRecognitionState("\u65e0\u969c\u788d\u670d\u52a1\u5df2\u4e2d\u65ad")
        }
    }

    override fun onDestroy() {
        scope.cancel()
        if (::stateStore.isInitialized) {
            stateStore.clearRecognitionState("\u65e0\u969c\u788d\u670d\u52a1\u5df2\u505c\u6b62")
        }
        super.onDestroy()
    }
}
