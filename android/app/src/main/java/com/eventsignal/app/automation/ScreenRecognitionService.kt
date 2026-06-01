package com.eventsignal.app.automation

import android.graphics.Rect
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
        val amountField = findAmountField(nodes, profile)
        val pageBottom = nodes.maxOfOrNull { node -> node.bounds.bottom } ?: 0
        val amountFieldFound = amountField != null
        val upButtonFound = findDirectionButton(nodes, profile.upButtonKeywords, amountField?.bounds, pageBottom) != null
        val downButtonFound = findDirectionButton(nodes, profile.downButtonKeywords, amountField?.bounds, pageBottom) != null
        val observedBalanceUsdt = extractObservedBalanceUsdt(texts)
        val observedIndexPrice = extractObservedIndexPrice(texts)
        val structure = detectEventContractStructure(texts, amountFieldFound, upButtonFound, downButtonFound)
        val confidence = pageConfidence(packageName, structure, profile)
        val isTargetPackage = isTargetPackage(packageName, profile)
        val isEventContractPage = structure.isEventContractPage ||
            (
                isTargetPackage &&
                    structure.hasSymbol &&
                    structure.hasAmountArea &&
                    structure.hasDirectionArea &&
                    (structure.hasTimeUnit || structure.hasEventTitle || structure.hasPaymentArea)
                )
        val amountReady = amountFieldFound || structure.hasAmountArea
        val directionReady = (upButtonFound && downButtonFound) || structure.hasDirectionArea

        return RecognizedScreen(
            packageName = packageName,
            screenName = inferScreenName(structure, texts, isEventContractPage),
            confidence = confidence,
            safeToAct = isEventContractPage &&
                amountReady &&
                directionReady &&
                confidence >= profile.minRecognitionConfidence,
            pageKeywordHits = pageKeywordHits,
            amountFieldFound = amountFieldFound,
            upButtonFound = upButtonFound,
            downButtonFound = downButtonFound,
            observedBalanceUsdt = observedBalanceUsdt,
            observedIndexPrice = observedIndexPrice,
            visibleText = texts.take(40),
        )
    }

    private fun inferScreenName(
        structure: EventContractStructure,
        texts: List<String>,
        isEventContractPage: Boolean,
    ): String =
        when {
            isEventContractPage -> "event_contract"
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
        val packageScore = if (isTargetPackage(packageName, profile)) {
            0.08
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

    private fun isTargetPackage(packageName: String, profile: AutomationProfile): Boolean =
        profile.targetPackageNames.isEmpty() ||
            profile.targetPackageNames.any { it.equals(packageName, ignoreCase = true) }

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
            allText.contains("合约") ||
            allText.contains("EVENTCONTRACT", ignoreCase = true)
        val hasSymbol = allText.contains("BTCUSDT", ignoreCase = true)
        val hasTimeUnit = allText.contains("时间单位") ||
            listOf("1分", "5分", "10分钟", "10分", "15分", "30分钟", "30分", "1小时", "10M", "10MIN", "30M", "30MIN").any { token ->
                allText.contains(token, ignoreCase = true)
            }
        val hasAmountArea = amountFieldFound ||
            allText.contains("数量") ||
            allText.contains("数量USDT") ||
            allText.contains("可用") ||
            allText.contains("USDT") ||
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
        editable.firstOrNull { node ->
            profile.amountFieldHints.any { hint -> node.textForMatching.contains(hint, ignoreCase = true) }
        }?.let { return it }
        editable.firstOrNull()?.let { return it }

        val amountLabelIndex = nodes.indexOfFirst { node -> isExplicitAmountLabel(node.textForMatching) }
        if (amountLabelIndex >= 0) {
            nodes.drop(amountLabelIndex + 1).take(12).firstOrNull { node ->
                node.isEnabled &&
                    (node.isEditable ||
                        node.isClickable ||
                        node.parentClickable ||
                        node.isFocusable ||
                        node.className.contains("EditText", ignoreCase = true)) &&
                    !node.textForMatching.contains("+") &&
                    !node.textForMatching.contains("上涨") &&
                    !node.textForMatching.contains("下跌")
            }?.let { return it }
        }
        return nodes.firstOrNull { node ->
            node.isEnabled &&
                (node.isEditable || node.isFocusable || node.className.contains("EditText", ignoreCase = true)) &&
                node.textForMatching.contains("USDT", ignoreCase = true)
        }
    }

    private fun isExplicitAmountLabel(text: String): Boolean =
        listOf("数量", "金额", "下注", "投入", "Amount", "Stake").any { keyword ->
            text.contains(keyword, ignoreCase = true)
        }

    private fun findDirectionButton(
        nodes: List<NodeSnapshot>,
        keywords: List<String>,
        amountBounds: Rect?,
        pageBottom: Int,
    ): NodeSnapshot? =
        nodes.firstOrNull { node ->
            node.isEnabled &&
                (node.isClickable || node.parentClickable) &&
                isInsideDirectionButtonZone(node.clickTargetBounds, amountBounds, pageBottom) &&
                !looksLikePayoutLabel(node.textForMatching) &&
                keywords.any { keyword -> node.textForMatching.contains(keyword, ignoreCase = true) }
        }

    private fun looksLikePayoutLabel(label: String): Boolean =
        label.contains("%") ||
            label.contains("支付率") ||
            label.contains("支付金额") ||
            label.contains("Payout", ignoreCase = true)

    private fun isInsideDirectionButtonZone(
        bounds: Rect,
        amountBounds: Rect?,
        pageBottom: Int,
    ): Boolean {
        if (bounds.isEmpty) return false
        if (bounds.width() < 120 || bounds.height() < 40) return false
        if (pageBottom > 0 && bounds.centerY() < (pageBottom * 0.30f).toInt()) return false
        if (amountBounds != null) {
            if (bounds.centerY() <= amountBounds.bottom + 24) return false
            if (bounds.centerY() > amountBounds.bottom + 520) return false
        }
        if (pageBottom > 0 && bounds.centerY() > pageBottom - 64) return false
        return true
    }

    private fun extractObservedBalanceUsdt(texts: List<String>): String? {
        val normalizedTexts = texts.map { it.replace(",", "").replace("\\s+".toRegex(), " ").trim() }
        val joined = normalizedTexts.joinToString(" ")
        val directPatterns = listOf(
            Regex("""可用\s*U\s*本\s*位\s*合\s*约\s*([0-9]+(?:\.[0-9]+)?)\s*USDT""", RegexOption.IGNORE_CASE),
            Regex("""U\s*本\s*位\s*合\s*约\s*([0-9]+(?:\.[0-9]+)?)\s*USDT""", RegexOption.IGNORE_CASE),
            Regex("""([0-9]+(?:\.[0-9]+)?)\s*USDT\s*U\s*本\s*位\s*合\s*约""", RegexOption.IGNORE_CASE),
        )
        directPatterns.firstNotNullOfOrNull { pattern ->
            pattern.find(joined)?.groupValues?.getOrNull(1)
        }?.let { return it }

        normalizedTexts.forEachIndexed { index, text ->
            if (text.contains("U本位合约") || text.contains("U 本位合约")) {
                val local = normalizedTexts.drop((index - 1).coerceAtLeast(0)).take(6).joinToString(" ")
                Regex("""([0-9]+(?:\.[0-9]+)?)\s*USDT""", RegexOption.IGNORE_CASE)
                    .find(local)
                    ?.groupValues
                    ?.getOrNull(1)
                    ?.let { return it }
            }
        }
        return null
    }

    private fun extractObservedIndexPrice(texts: List<String>): String? {
        val normalizedTexts = texts.map {
            it.replace(",", "").replace("\\s+".toRegex(), " ").trim()
        }
        val btcPrice = Regex("""\b([5-9][0-9]{4}(?:\.[0-9]+)?)\b""")
        return normalizedTexts
            .asSequence()
            .flatMap { text -> btcPrice.findAll(text).map { match -> match.groupValues[1] } }
            .firstOrNull()
    }

    private fun AccessibilityNodeInfo.toSnapshot(): NodeSnapshot =
        NodeSnapshot(
            textForMatching = listOfNotNull(
                text?.toString(),
                contentDescription?.toString(),
                hintText?.toString(),
                viewIdResourceName,
            ).joinToString(" "),
            className = className?.toString().orEmpty(),
            isEditable = isEditable,
            isEnabled = isEnabled,
            isClickable = isClickable,
            isFocusable = isFocusable,
            parentClickable = parent?.isClickable == true,
            bounds = boundsInScreen(),
            clickTargetBounds = clickTargetBounds(),
        )

    private fun String.normalized(): String =
        replace("\\s+".toRegex(), "")

    private fun AccessibilityNodeInfo.boundsInScreen(): Rect =
        Rect().also { getBoundsInScreen(it) }

    private fun AccessibilityNodeInfo.clickTargetBounds(): Rect {
        if (isClickable) return boundsInScreen()
        var parentNode = parent
        while (parentNode != null) {
            if (parentNode.isClickable) return parentNode.boundsInScreen()
            parentNode = parentNode.parent
        }
        return boundsInScreen()
    }
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
        hasSymbol && hasTimeUnit && hasAmountArea && hasDirectionArea && (hasEventTitle || hasPaymentArea)
}

private data class NodeSnapshot(
    val textForMatching: String,
    val className: String,
    val isEditable: Boolean,
    val isEnabled: Boolean,
    val isClickable: Boolean,
    val isFocusable: Boolean,
    val parentClickable: Boolean,
    val bounds: Rect,
    val clickTargetBounds: Rect,
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
    val observedBalanceUsdt: String?,
    val observedIndexPrice: String?,
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
            observedBalanceUsdt = null,
            observedIndexPrice = null,
            visibleText = emptyList(),
        )
    }
}
