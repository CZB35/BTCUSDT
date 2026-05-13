package com.eventsignal.app.automation

import android.accessibilityservice.AccessibilityService
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
        if (settings.executionMode == "manual" || !settings.automationEnabled) return

        val signalId = signal.id ?: run {
            stateStore.saveLastStatus("missing_signal_id")
            return
        }
        if (!signal.tradeAuthorized) {
            stateStore.saveLastStatus("signal_not_authorized:${signal.authorizationStatus ?: "-"}")
            return
        }
        if (stateStore.hasExecuted(signalId)) {
            stateStore.saveLastStatus("signal_already_executed:$signalId")
            return
        }

        val amount = signal.suggestedStake?.trim()?.takeIf { it.isNotBlank() } ?: run {
            stateStore.saveLastStatus("missing_suggested_stake")
            return
        }
        val direction = signal.direction.uppercase()
        if (direction != "UP" && direction != "DOWN") {
            stateStore.saveLastStatus("unsupported_direction:$direction")
            return
        }

        val root = service.rootInActiveWindow ?: run {
            stateStore.saveLastStatus("missing_root_window")
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
                lastAction = "识别完成",
                visibleText = screen.visibleText.take(12),
            ),
        )
        if (amountField == null) {
            stateStore.saveLastStatus("amount_field_not_found")
            return
        }
        if (!setText(amountField, amount)) {
            saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "金额输入失败")
            stateStore.saveLastStatus("amount_input_failed")
            return
        }

        if (settings.executionMode == "assisted" || settings.requireFinalConfirmation) {
            saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "已输入金额，等待手动确认")
            stateStore.saveLastStatus("amount_prepared_waiting_confirmation:$amount")
            return
        }

        val button = if (direction == "UP") upButton else downButton
        if (button == null) {
            saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "未找到${if (direction == "UP") "看涨" else "看跌"}按钮")
            stateStore.saveLastStatus("direction_button_not_found:$direction")
            return
        }
        if (click(button)) {
            saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "已点击${if (direction == "UP") "看涨" else "看跌"}，等待确认")
            if (clickConfirm(profile)) {
                stateStore.markExecuted(signalId)
                saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "已确认${if (direction == "UP") "看涨" else "看跌"}")
                submitOrderReport(
                    signal = signal,
                    signalId = signalId,
                    direction = direction,
                    amount = amount,
                    profile = profile,
                    screen = screen,
                )
                stateStore.saveLastStatus("confirmed:$direction:$amount:${screen.confidence}")
            } else {
                stateStore.saveLastStatus("confirm_button_not_found:$direction:$amount")
                saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "未找到确认按钮")
            }
        } else {
            saveActionState(screen, amountFieldFound = true, upButton != null, downButton != null, "按钮点击失败")
            stateStore.saveLastStatus("direction_click_failed:$direction")
        }
    }

    private fun submitOrderReport(
        signal: SignalSnapshot,
        signalId: String,
        direction: String,
        amount: String,
        profile: AutomationProfile,
        screen: RecognizedScreen,
    ) {
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
        repository.submitEventContractOrder(order)
            .onSuccess { stateStore.saveLastStatus("order_reported:$direction:$amount") }
            .onFailure { stateStore.saveLastStatus("order_report_failed:${it.message}") }
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
        if (editable.isEmpty()) return null

        return editable.firstOrNull { node ->
            val label = node.nodeText()
            profile.amountFieldHints.any { hint -> label.contains(hint, ignoreCase = true) }
        } ?: editable.firstOrNull()
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
