package com.eventsignal.app.automation

import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo

class ScreenRecognitionService {
    fun recognize(
        root: AccessibilityNodeInfo?,
        event: AccessibilityEvent?,
        profile: AutomationProfile,
    ): RecognizedScreen {
        if (root == null) return RecognizedScreen.Unknown

        val packageName = event?.packageName?.toString().orEmpty().ifBlank {
            root.packageName?.toString().orEmpty()
        }
        val nodes = collectNodes(root)
        val texts = nodes.map { it.textForMatching }.filter { it.isNotBlank() }.distinct()
        val pageKeywordHits = matchedKeywords(texts, profile.pageKeywords)
        val amountFieldFound = findAmountField(nodes, profile) != null
        val upButtonFound = findDirectionButton(nodes, profile.upButtonKeywords) != null
        val downButtonFound = findDirectionButton(nodes, profile.downButtonKeywords) != null
        val structure = detectEventContractStructure(texts, amountFieldFound, upButtonFound, downButtonFound)
        val confidence = pageConfidence(packageName, structure, profile)

        return RecognizedScreen(
            packageName = packageName,
            screenName = inferScreenName(structure, texts),
            confidence = confidence,
            safeToAct = structure.isEventContractPage && confidence >= profile.minRecognitionConfidence,
            pageKeywordHits = pageKeywordHits,
            amountFieldFound = amountFieldFound,
            upButtonFound = upButtonFound,
            downButtonFound = downButtonFound,
            visibleText = texts.take(40),
        )
    }

    private fun inferScreenName(structure: EventContractStructure, texts: List<String>): String =
        when {
            structure.isEventContractPage -> "event_contract"
            structure.hasEventTitle -> "possible_event_contract"
            texts.any { it.contains("BTC", ignoreCase = true) } -> "possible_btc_contract"
            texts.any { it.contains("USDT", ignoreCase = true) } -> "possible_balance_or_amount"
            else -> "unknown"
        }

    private fun pageConfidence(
        packageName: String,
        structure: EventContractStructure,
        profile: AutomationProfile,
    ): Double {
        val packageScore = if (
            profile.targetPackageNames.isEmpty() ||
            profile.targetPackageNames.any { it.equals(packageName, ignoreCase = true) }
        ) {
            0.04
        } else {
            0.02
        }
        val structureScore =
            (if (structure.hasEventTitle) 0.30 else 0.0) +
                (if (structure.hasSymbol) 0.14 else 0.0) +
                (if (structure.hasTimeUnit) 0.12 else 0.0) +
                (if (structure.hasAmountArea) 0.16 else 0.0) +
                (if (structure.hasDirectionArea) 0.20 else 0.0) +
                (if (structure.hasPaymentArea) 0.04 else 0.0)
        return (packageScore + structureScore).coerceIn(0.0, 1.0)
    }

    private fun matchedKeywords(texts: List<String>, keywords: List<String>): List<String> =
        keywords.filter { keyword ->
            texts.any { text -> text.contains(keyword, ignoreCase = true) }
        }

    private fun detectEventContractStructure(
        texts: List<String>,
        amountFieldFound: Boolean,
        upButtonFound: Boolean,
        downButtonFound: Boolean,
    ): EventContractStructure {
        val normalizedTexts = texts.map { it.normalized() }
        val allText = normalizedTexts.joinToString(" ")
        val hasEventTitle = allText.contains("事件合约") ||
            allText.contains("EVENTCONTRACT", ignoreCase = true)
        val hasSymbol = allText.contains("BTCUSDT", ignoreCase = true)
        val hasTimeUnit = allText.contains("时间单位") ||
            listOf("1分", "5分", "10分钟", "15分", "30分", "1小时", "10M").any { token ->
                allText.contains(token, ignoreCase = true)
            }
        val hasAmountArea = amountFieldFound ||
            allText.contains("数量") ||
            allText.contains("AMOUNT", ignoreCase = true) ||
            allText.contains("STAKE", ignoreCase = true)
        val hasDirectionArea = (upButtonFound && downButtonFound) ||
            ((allText.contains("上涨") || allText.contains("看涨")) &&
                (allText.contains("下跌") || allText.contains("看跌"))) ||
            (allText.contains("UP", ignoreCase = true) && allText.contains("DOWN", ignoreCase = true))
        val hasPaymentArea = allText.contains("支付率") ||
            allText.contains("支付金额") ||
            allText.contains("PAYOUT", ignoreCase = true)

        return EventContractStructure(
            hasEventTitle = hasEventTitle,
            hasSymbol = hasSymbol,
            hasTimeUnit = hasTimeUnit,
            hasAmountArea = hasAmountArea,
            hasDirectionArea = hasDirectionArea,
            hasPaymentArea = hasPaymentArea,
        )
    }

    private fun collectNodes(root: AccessibilityNodeInfo): List<NodeSnapshot> {
        val output = mutableListOf<NodeSnapshot>()
        fun visit(node: AccessibilityNodeInfo) {
            output.add(node.toSnapshot())
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

    private fun findAmountField(nodes: List<NodeSnapshot>, profile: AutomationProfile): NodeSnapshot? {
        val editable = nodes.filter { it.isEditable && it.isEnabled }
        if (editable.isEmpty()) return null

        return editable.firstOrNull { node ->
            profile.amountFieldHints.any { hint -> node.textForMatching.contains(hint, ignoreCase = true) }
        } ?: editable.firstOrNull()
    }

    private fun findDirectionButton(nodes: List<NodeSnapshot>, keywords: List<String>): NodeSnapshot? =
        nodes.firstOrNull { node ->
            node.isEnabled &&
                (node.isClickable || node.parentClickable) &&
                keywords.any { keyword -> node.textForMatching.contains(keyword, ignoreCase = true) }
        }

    private fun AccessibilityNodeInfo.toSnapshot(): NodeSnapshot =
        NodeSnapshot(
            textForMatching = listOfNotNull(
                text?.toString(),
                contentDescription?.toString(),
                hintText?.toString(),
                viewIdResourceName,
            ).joinToString(" "),
            isEditable = isEditable,
            isEnabled = isEnabled,
            isClickable = isClickable,
            parentClickable = parent?.isClickable == true,
        )

    private fun String.normalized(): String =
        replace("\\s+".toRegex(), "")
}

private data class EventContractStructure(
    val hasEventTitle: Boolean,
    val hasSymbol: Boolean,
    val hasTimeUnit: Boolean,
    val hasAmountArea: Boolean,
    val hasDirectionArea: Boolean,
    val hasPaymentArea: Boolean,
) {
    val isEventContractPage: Boolean =
        hasEventTitle && hasSymbol && hasTimeUnit && hasAmountArea && hasDirectionArea
}

private data class NodeSnapshot(
    val textForMatching: String,
    val isEditable: Boolean,
    val isEnabled: Boolean,
    val isClickable: Boolean,
    val parentClickable: Boolean,
)

data class RecognizedScreen(
    val packageName: String,
    val screenName: String,
    val confidence: Double,
    val safeToAct: Boolean,
    val pageKeywordHits: List<String>,
    val amountFieldFound: Boolean,
    val upButtonFound: Boolean,
    val downButtonFound: Boolean,
    val visibleText: List<String>,
) {
    companion object {
        val Unknown = RecognizedScreen(
            packageName = "",
            screenName = "unknown",
            confidence = 0.0,
            safeToAct = false,
            pageKeywordHits = emptyList(),
            amountFieldFound = false,
            upButtonFound = false,
            downButtonFound = false,
            visibleText = emptyList(),
        )
    }
}
