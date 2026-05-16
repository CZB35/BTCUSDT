package com.eventsignal.app.automation

import android.accessibilityservice.AccessibilityService
import android.view.accessibility.AccessibilityNodeInfo

fun AccessibilityService.findAutomationRoot(profile: AutomationProfile): AccessibilityNodeInfo? {
    val candidates = mutableListOf<AccessibilityNodeInfo>()
    runCatching {
        windows.orEmpty().mapNotNullTo(candidates) { window -> window.root }
    }
    rootInActiveWindow?.let { activeRoot -> candidates.add(activeRoot) }
    if (candidates.isEmpty()) return null

    return candidates.maxByOrNull { root -> automationRootScore(root, profile) }
}

private fun automationRootScore(root: AccessibilityNodeInfo, profile: AutomationProfile): Int {
    val packageName = root.packageName?.toString().orEmpty()
    val text = collectWindowText(root).joinToString(" ").normalizedForWindowScore()
    var score = 0

    if (profile.targetPackageNames.any { it.equals(packageName, ignoreCase = true) }) score += 100
    if (packageName.startsWith("com.eventsignal.app", ignoreCase = true)) score -= 120

    if (text.contains("BTCUSDT", ignoreCase = true)) score += 35
    if (text.contains("事件合约") || text.contains("EVENTCONTRACT", ignoreCase = true)) score += 25
    if (text.contains("10分钟") || text.contains("10M", ignoreCase = true)) score += 18
    if (text.contains("数量") || text.contains("数量USDT") || text.contains("AMOUNT", ignoreCase = true)) score += 16
    if (text.contains("可用") || text.contains("U本位合约") || text.contains("USDT", ignoreCase = true)) score += 12
    if (text.contains("支付率") || text.contains("支付金额") || text.contains("PAYOUT", ignoreCase = true)) score += 12
    if ((text.contains("上涨") || text.contains("看涨") || text.contains("UP", ignoreCase = true)) &&
        (text.contains("下跌") || text.contains("看跌") || text.contains("DOWN", ignoreCase = true))
    ) {
        score += 22
    }

    return score
}

private fun collectWindowText(root: AccessibilityNodeInfo): List<String> {
    val output = mutableListOf<String>()
    fun visit(node: AccessibilityNodeInfo) {
        if (output.size >= 120) return
        listOfNotNull(
            node.text?.toString(),
            node.contentDescription?.toString(),
            node.hintText?.toString(),
            node.viewIdResourceName,
        ).joinToString(" ")
            .trim()
            .takeIf { it.isNotBlank() }
            ?.let(output::add)
        for (index in 0 until node.childCount) {
            node.getChild(index)?.let { child ->
                visit(child)
                child.recycle()
            }
        }
    }
    visit(root)
    return output
}

private fun String.normalizedForWindowScore(): String =
    replace("\\s+".toRegex(), "")
