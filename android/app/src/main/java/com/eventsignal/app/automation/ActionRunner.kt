package com.eventsignal.app.automation

import android.accessibilityservice.AccessibilityService
import android.graphics.Rect
import android.os.Bundle
import android.os.SystemClock
import android.view.accessibility.AccessibilityNodeInfo
import com.eventsignal.app.data.EventSignalRepository
import com.eventsignal.app.domain.AutomationSettings
import com.eventsignal.app.domain.EventContractOrderRequest
import com.eventsignal.app.domain.SignalSnapshot
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

        var root = service.findAutomationRoot(profile) ?: run {
            stateStore.saveLastStatus("missing_root_window")
            releaseReservation(signal, "missing_root_window")
            return
        }
        if (!ensurePeriodSelected(root, profile, signal.periodSeconds)) {
            stateStore.saveLastStatus("period_select_failed:${signal.periodSeconds}")
            releaseReservation(signal, "period_select_failed")
            return
        }
        root = service.findAutomationRoot(profile) ?: run {
            stateStore.saveLastStatus("missing_root_window_after_period_select")
            releaseReservation(signal, "missing_root_window_after_period_select")
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

        SystemClock.sleep(300)
        val refreshedRoot = service.findAutomationRoot(profile) ?: run {
            saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "金额已输入，页面丢失")
            stateStore.saveLastStatus("missing_root_window_after_input")
            releaseReservation(signal, "missing_root_window_after_input")
            return
        }
        val refreshedNodes = flatten(refreshedRoot)
        val refreshedUpButton = findDirectionButton(refreshedNodes, profile, "UP")
        val refreshedDownButton = findDirectionButton(refreshedNodes, profile, "DOWN")
        saveActionState(
            screen,
            amountFieldFound = true,
            upButtonFound = refreshedUpButton != null,
            downButtonFound = refreshedDownButton != null,
            action = "金额已输入，重新识别执行按钮",
        )

        val button = if (direction == "UP") refreshedUpButton else refreshedDownButton
        if (button == null) {
            saveActionState(screen, amountFieldFound = true, refreshedUpButton != null, refreshedDownButton != null, "未找到执行按钮")
            stateStore.saveLastStatus("action_button_not_found")
            releaseReservation(signal, "action_button_not_found")
            return
        }
        val buttonLabel = button.nodeText().take(40)
        if (click(button)) {
            saveActionState(screen, amountFieldFound = true, refreshedUpButton != null, refreshedDownButton != null, "已点击执行按钮:$buttonLabel")
            if (clickConfirm(profile)) {
                saveActionState(screen, amountFieldFound = true, refreshedUpButton != null, refreshedDownButton != null, "已确认下单")
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
                saveActionState(screen, amountFieldFound = true, refreshedUpButton != null, refreshedDownButton != null, "未找到确认按钮:$buttonLabel")
            }
        } else {
            saveActionState(screen, amountFieldFound = true, refreshedUpButton != null, refreshedDownButton != null, "按钮点击失败:$buttonLabel")
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
            "fee_reserve_insufficient" -> stateStore.saveLastStatus("signal_not_authorized_fee_reserve_insufficient")
            "mismatch",
            "observed_balance_missing",
            "exchange_balance_missing",
            "exchange_balance_stale",
            "page_state_missing",
            "page_state_stale",
            "page_not_ready",
            "unknown",
            -> stateStore.saveLastStatus("signal_not_authorized_account_gate:${signal.authorizationStatus}")
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
            periodSeconds = signal.periodSeconds,
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

    private fun ensurePeriodSelected(
        root: AccessibilityNodeInfo,
        profile: AutomationProfile,
        periodSeconds: Int,
    ): Boolean {
        val targetLabels = periodLabels(periodSeconds) ?: return true
        val nodes = flatten(root)
        val timeUnitIndex = nodes.indexOfFirst { node ->
            node.nodeText().matchesAnyCompact(listOf("时间单位", "Time Unit", "Duration"))
        }
        if (timeUnitIndex < 0) return false
        val selectorSearchNodes = if (timeUnitIndex >= 0) {
            nodes.drop(timeUnitIndex).take(12)
        } else {
            nodes
        }
        if (selectorSearchNodes.any { node ->
                node.isVisibleToUser && node.nodeText().matchesAnyCompact(targetLabels)
            }) {
            return true
        }
        val selector = selectorSearchNodes
            .filter { node ->
                val label = node.nodeText()
                node.isEnabled &&
                    node.isVisibleToUser &&
                    (node.isClickable || node.parent?.isClickable == true) &&
                    label.matchesAnyCompact(periodSelectorLabels)
            }
            .minByOrNull { node -> node.boundsInScreen().centerY() }
            ?: return false

        if (!click(selector)) return false
        repeat(8) {
            SystemClock.sleep(250)
            val optionRoot = service.findAutomationRoot(profile) ?: return@repeat
            val option = flatten(optionRoot).firstOrNull { node ->
                val label = node.nodeText()
                node.isEnabled &&
                    node.isVisibleToUser &&
                    (node.isClickable || node.parent?.isClickable == true) &&
                    label.matchesAnyCompact(targetLabels)
            }
            if (option != null && click(option)) {
                SystemClock.sleep(300)
                return true
            }
        }
        return false
    }

    private fun periodLabels(periodSeconds: Int): List<String>? =
        when (periodSeconds) {
            60 -> listOf("1分钟", "1分", "1m", "1min")
            300 -> listOf("5分钟", "5分", "5m", "5min")
            600 -> listOf("10分钟", "10分", "10m", "10min")
            900 -> listOf("15分钟", "15分", "15m", "15min")
            1800 -> listOf("30分钟", "30分", "30m", "30min")
            3600 -> listOf("1小时", "1h", "60分钟", "60min")
            else -> null
        }

    private val periodSelectorLabels = listOf(
        "1分钟",
        "5分钟",
        "10分钟",
        "15分钟",
        "30分钟",
        "1小时",
        "1分",
        "5分",
        "10分",
        "15分",
        "30分",
        "1m",
        "5m",
        "10m",
        "15m",
        "30m",
        "1min",
        "5min",
        "10min",
        "15min",
        "30min",
    )

    private fun String.matchesAnyCompact(options: List<String>): Boolean {
        val value = compactPeriodText()
        return options.any { option -> value.contains(option.compactPeriodText()) }
    }

    private fun String.compactPeriodText(): String =
        replace("\\s+".toRegex(), "").uppercase()

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
        return nodes
            .filter { node -> isDirectionButtonCandidate(node, keywords) }
            .maxByOrNull { node -> directionButtonScore(node) }
    }

    private fun isDirectionButtonCandidate(
        node: AccessibilityNodeInfo,
        keywords: List<String>,
    ): Boolean {
        val label = node.nodeText()
        if (looksLikePayoutLabel(label)) return false
        return node.isEnabled &&
            (node.isClickable || node.parent?.isClickable == true) &&
            keywords.any { keyword -> label.contains(keyword, ignoreCase = true) }
    }

    private fun looksLikePayoutLabel(label: String): Boolean =
        (label.contains("%") || label.contains("支付率") || label.contains("Payout", ignoreCase = true)) &&
            !label.contains("↗") &&
            !label.contains("↘")

    private fun directionButtonScore(node: AccessibilityNodeInfo): Int {
        val label = node.nodeText()
        val bounds = node.boundsInScreen()
        var score = 0
        if (node.isClickable) score += 4
        if (label.contains("%")) score -= 35
        if (label.contains("支付率") || label.contains("支付金额") || label.contains("Payout", ignoreCase = true)) {
            score -= 25
        }
        if (label.contains("↗") || label.contains("↘")) score += 10
        if (label.contains("看涨") || label.contains("看跌")) score += 4
        if (label.length <= 12) score += 4
        if (bounds.width() >= 160) score += 8
        if (bounds.height() >= 48) score += 8
        if (bounds.width() >= 240 && bounds.height() >= 60) score += 8
        score += (bounds.centerY() / 80).coerceIn(0, 40)
        return score
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
        repeat(20) {
            SystemClock.sleep(250)
            val root = service.findAutomationRoot(profile) ?: return@repeat
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

    private fun AccessibilityNodeInfo.boundsInScreen(): Rect =
        Rect().also { getBoundsInScreen(it) }
}
