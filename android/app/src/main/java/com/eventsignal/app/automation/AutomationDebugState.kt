package com.eventsignal.app.automation

import kotlinx.serialization.Serializable

@Serializable
data class AutomationDebugState(
    val accessibilityConnected: Boolean = false,
    val lastEventAtMs: Long = 0,
    val packageName: String = "-",
    val screenName: String = "-",
    val confidence: Double = 0.0,
    val safeToAct: Boolean = false,
    val pageKeywordHits: List<String> = emptyList(),
    val amountFieldFound: Boolean = false,
    val upButtonFound: Boolean = false,
    val downButtonFound: Boolean = false,
    val lastAction: String = "-",
    val visibleText: List<String> = emptyList(),
    val updatedAtMs: Long = 0,
)
