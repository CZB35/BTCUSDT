package com.eventsignal.app.automation

import android.accessibilityservice.AccessibilityService
import android.os.Bundle
import android.os.SystemClock
import android.view.accessibility.AccessibilityNodeInfo
import com.eventsignal.app.data.EventSignalRepository
import com.eventsignal.app.domain.AutomationSettings
import com.eventsignal.app.domain.EventContractOrderRequest
import com.eventsignal.app.domain.SignalSnapshot
import com.eventsignal.app.service.AutomationRuntime
import java.time.Instant

class ActionRunner(
    private val service: AccessibilityService,
    private val stateStore: AutomationStateStore,
) {
    private val repository = EventSignalRepository(service.applicationContext)

    fun maybeRun(
        screen: RecognizedScreen,
        profile: AutomationProfile,
        settings: AutomationSettings,
        signal: SignalSnapshot,
    ) {
        if (!screen.safeToAct) return
        if (!settings.automationEnabled || settings.executionMode != "automatic") return

        val signalId = signal.id ?: run {
            stateStore.saveLastStatus("missing_signal_id")
            return
        }
        if (!signal.tradeAuthorized) {
            pauseForUnauthorizedSignal(signal)
            return
        }
        if (stateStore.hasExecuted(signalId)) {
            stateStore.saveLastStatus("signal_already_executed:$signalId")
            return
        }

        val amount = signal.suggestedStake?.trim()?.takeIf { it.isNotBlank() } ?: run {
            stateStore.saveLastStatus("missing_suggested_stake")
            releaseReservation(signal, "missing_suggested_stake")
            return
        }
        val direction = signal.direction.uppercase()
        if (direction != "UP" && direction != "DOWN") {
            stateStore.saveLastStatus("unsupported_direction:$direction")
            releaseReservation(signal, "unsupported_direction")
            return
        }

        val root = service.rootInActiveWindow ?: run {
            stateStore.saveLastStatus("missing_root_window")
            releaseReservation(signal, "missing_root_window")
            return
        }
        val nodes = flatten(root)
        val amountField = findAmountField(nodes, profile)
        val upButton = findDirectionButton(nodes, profile, "UP")
        val downButton = findDirectionButton(nodes, profile, "DOWN")
        stateStore.saveDebugState(
            AutomationDebugState(
                accessibilityConnected = true,
                lastEventAtMs = System.currentTimeMillis(),
                packageName = screen.packageName,
                screenName = screen.screenName,
                confidence = screen.confidence,
                safeToAct = screen.safeToAct,
                pageKeywordHits = screen.pageKeywordHits,
                amountFieldFound = amountField != null,
                upButtonFound = upButton != null,
                downButtonFound = downButton != null,
                observedBalanceUsdt = screen.observedBalanceUsdt,
                lastAction = "识别完成",
                visibleText = screen.visibleText.take(12),
            ),
        )
        if (amountField == null) {
            stateStore.saveLastStatus("amount_field_not_found")
            releaseReservation(signal, "amount_field_not_found")
            return
        }
        if (!setText(amountField, amount)) {
            saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "金额输入失败")
            stateStore.saveLastStatus("amount_input_failed")
            releaseReservation(signal, "amount_input_failed")
            return
        }

        val button = if (direction == "UP") upButton else downButton
        if (button == null) {
            saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "未找到执行按钮")
            stateStore.saveLastStatus("action_button_not_found")
            releaseReservation(signal, "action_button_not_found")
            return
        }
        if (click(button)) {
            saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "已点击执行按钮，等待确认")
            if (clickConfirm(profile)) {
                saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "已确认下单")
                val pending = buildOrderReport(
                    signal = signal,
                    signalId = signalId,
                    direction = direction,
                    amount = amount,
                    profile = profile,
                    screen = screen,
                )
                stateStore.savePendingOrder(pending)
                submitPendingOrder(pending)
                stateStore.saveLastStatus("confirmed:$amount:${screen.confidence}")
            } else {
                stateStore.saveLastStatus("confirm_button_not_found:$amount")
                releaseReservation(signal, "confirm_button_not_found")
                saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "未找到确认按钮")
            }
        } else {
            saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "按钮点击失败")
            stateStore.saveLastStatus("action_click_failed")
            releaseReservation(signal, "action_click_failed")
        }
    }

    private fun releaseReservation(signal: SignalSnapshot, reason: String) {
        val reservationId = signal.reservationId?.trim()?.takeIf { it.isNotBlank() } ?: return
        stateStore.markExecuted(signal.id ?: reservationId)
        repository.releaseSignalReservation(reservationId, reason)
            .onFailure {
                stateStore.saveLastStatus("reservation_release_failed:${it.javaClass.simpleName}")
            }
    }

    private fun pauseForUnauthorizedSignal(signal: SignalSnapshot) {
        when (signal.authorizationStatus) {
            "fee_reserve_insufficient" -> {
                AutomationRuntime.stop()
                stateStore.saveLastStatus("signal_not_authorized_fee_reserve_insufficient")
            }
            "mismatch",
            "observed_balance_missing",
            "exchange_balance_missing",
            "exchange_balance_stale",
            "unknown",
            -> {
                AutomationRuntime.stop()
                stateStore.saveLastStatus("signal_not_authorized_account_gate")
            }
            else -> stateStore.saveLastStatus("signal_not_authorized")
        }
    }

    private fun buildOrderReport(
        signal: SignalSnapshot,
        signalId: String,
        direction: String,
        amount: String,
        profile: AutomationProfile,
        screen: RecognizedScreen,
    ): PendingOrderReport {
        val traceId = "android-${System.currentTimeMillis()}-$signalId"
        val order = EventContractOrderRequest(
            clientOrderId = traceId,
            source = "android_automation",
            reservationId = signal.reservationId,
            signalId = signalId,
            symbol = signal.symbol,
            periodSeconds = 600,
            direction = direction,
            stakeUsdt = amount,
            payoutRatio = null,
            expectedPayoutUsdt = null,
            expectedProfitUsdt = null,
            automationProfile = profile.id,
            automationConfidence = "%.6f".format(screen.confidence),
            actionTraceId = traceId,
            rawAudit = mapOf(
                "screen_name" to screen.screenName,
                "amount_field_found" to screen.amountFieldFound.toString(),
                "up_button_found" to screen.upButtonFound.toString(),
                "down_button_found" to screen.downButtonFound.toString(),
            ),
            reportedAt = Instant.now().toString(),
        )
        return PendingOrderReport(
            signalId = signalId,
            clientOrderId = traceId,
            order = order,
            createdAtMs = System.currentTimeMillis(),
        )
    }

    private fun submitPendingOrder(pending: PendingOrderReport) {
        repository.submitEventContractOrder(pending.order)
            .onSuccess {
                stateStore.markExecuted(pending.signalId)
                stateStore.removePendingOrder(pending.clientOrderId)
                stateStore.saveLastStatus("order_reported:${pending.order.stakeUsdt}")
            }
            .onFailure {
                stateStore.savePendingOrder(pending.nextAttempt())
                stateStore.saveLastStatus("order_report_failed:${it.javaClass.simpleName}")
            }
    }

    private fun saveActionState(
        screen: RecognizedScreen,
        amountFieldFound: Boolean,
        upButtonFound: Boolean,
        downButtonFound: Boolean,
        action: String,
    ) {
        stateStore.saveDebugState(
            AutomationDebugState(
                accessibilityConnected = true,
                lastEventAtMs = System.currentTimeMillis(),
                packageName = screen.packageName,
                screenName = screen.screenName,
                confidence = screen.confidence,
                safeToAct = screen.safeToAct,
                pageKeywordHits = screen.pageKeywordHits,
                amountFieldFound = amountFieldFound,
                upButtonFound = upButtonFound,
                downButtonFound = downButtonFound,
                observedBalanceUsdt = screen.observedBalanceUsdt,
                lastAction = action,
                visibleText = screen.visibleText.take(12),
            ),
        )
    }

    private fun findAmountField(
        nodes: List<AccessibilityNodeInfo>,
        profile: AutomationProfile,
    ): AccessibilityNodeInfo? {
        val editable = nodes.filter { it.isEditable && it.isEnabled }
        editable.firstOrNull { node ->
            val label = node.nodeText()
            profile.amountFieldHints.any { hint -> label.contains(hint, ignoreCase = true) }
        }?.let { return it }
        editable.firstOrNull()?.let { return it }

        val amountLabelIndex = nodes.indexOfFirst { node ->
            val label = node.nodeText()
            profile.amountFieldHints.any { hint -> label.contains(hint, ignoreCase = true) }
        }
        if (amountLabelIndex >= 0) {
            nodes.drop(amountLabelIndex + 1).take(14).firstOrNull { node ->
                node.isEnabled &&
                    (node.isEditable || node.isClickable || node.isFocusable || node.className?.contains("EditText", ignoreCase = true) == true) &&
                    !node.nodeText().contains("+") &&
                    !node.nodeText().contains("上涨") &&
                    !node.nodeText().contains("下跌")
            }?.let { return it }
        }
        return nodes.firstOrNull { node ->
            node.isEnabled &&
                (node.isEditable || node.isFocusable || node.className?.contains("EditText", ignoreCase = true) == true) &&
                node.nodeText().contains("USDT", ignoreCase = true)
        }
    }

    private fun findDirectionButton(
        nodes: List<AccessibilityNodeInfo>,
        profile: AutomationProfile,
        direction: String,
    ): AccessibilityNodeInfo? {
        val keywords = if (direction == "UP") profile.upButtonKeywords else profile.downButtonKeywords
        return nodes.firstOrNull { node ->
            node.isEnabled &&
                (node.isClickable || node.parent?.isClickable == true) &&
                keywords.any { keyword -> node.nodeText().contains(keyword, ignoreCase = true) }
        }
    }

    private fun setText(node: AccessibilityNodeInfo, value: String): Boolean {
        val args = Bundle().apply {
            putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, value)
        }
        if (node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)) return true
        click(node)
        return node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)
    }

    private fun click(node: AccessibilityNodeInfo): Boolean {
        if (node.performAction(AccessibilityNodeInfo.ACTION_CLICK)) return true
        var parent = node.parent
        while (parent != null) {
            if (parent.isClickable && parent.performAction(AccessibilityNodeInfo.ACTION_CLICK)) {
                return true
            }
            parent = parent.parent
        }
        return false
    }

    private fun clickConfirm(profile: AutomationProfile): Boolean {
        repeat(12) {
            SystemClock.sleep(250)
            val root = service.rootInActiveWindow ?: return@repeat
            val confirm = findButton(flatten(root), profile.confirmButtonKeywords)
            if (confirm != null && click(confirm)) {
                return true
            }
        }
        return false
    }

    private fun findButton(
        nodes: List<AccessibilityNodeInfo>,
        keywords: List<String>,
    ): AccessibilityNodeInfo? =
        nodes.firstOrNull { node ->
            node.isEnabled &&
                (node.isClickable || node.parent?.isClickable == true) &&
                keywords.any { keyword -> node.nodeText().contains(keyword, ignoreCase = true) }
        }

    private fun flatten(root: AccessibilityNodeInfo): List<AccessibilityNodeInfo> {
        val nodes = mutableListOf<AccessibilityNodeInfo>()
        fun visit(node: AccessibilityNodeInfo) {
            nodes.add(node)
            for (index in 0 until node.childCount) {
                node.getChild(index)?.let(::visit)
            }
        }
        visit(root)
        return nodes
    }

    private fun AccessibilityNodeInfo.nodeText(): String =
        listOfNotNull(
            text?.toString(),
            contentDescription?.toString(),
            hintText?.toString(),
            viewIdResourceName,
        ).joinToString(" ")
}
