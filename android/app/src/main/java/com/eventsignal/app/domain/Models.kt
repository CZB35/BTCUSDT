package com.eventsignal.app.domain

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class AutomationSettings(
    @SerialName("automation_enabled")
    val automationEnabled: Boolean,
    @SerialName("execution_mode")
    val executionMode: String,
    @SerialName("target_app_profile")
    val targetAppProfile: String?,
    @SerialName("require_final_confirmation")
    val requireFinalConfirmation: Boolean,
    @SerialName("emergency_stop_enabled")
    val emergencyStopEnabled: Boolean,
) {
    companion object {
        fun default() = AutomationSettings(
            automationEnabled = false,
            executionMode = "manual",
            targetAppProfile = "binance_event_contract_v1",
            requireFinalConfirmation = true,
            emergencyStopEnabled = true,
        )
    }
}

@Serializable
data class AuthRequest(
    @SerialName("wallet_address")
    val walletAddress: String,
    val password: String,
    @SerialName("device_fingerprint")
    val deviceFingerprint: String,
)

@Serializable
data class AuthResponse(
    @SerialName("access_token")
    val accessToken: String,
    @SerialName("refresh_token")
    val refreshToken: String,
    @SerialName("expires_in")
    val expiresIn: Long,
)

@Serializable
data class SignalSnapshot(
    val id: String? = null,
    val symbol: String = "BTCUSDT",
    val direction: String = "HOLD",
    val confidence: String = "-",
    @SerialName("suggested_stake")
    val suggestedStake: String? = null,
    @SerialName("trade_authorized")
    val tradeAuthorized: Boolean = false,
    @SerialName("reservation_id")
    val reservationId: String? = null,
    @SerialName("fee_reserve_usdt")
    val feeReserveUsdt: String? = null,
    @SerialName("authorization_status")
    val authorizationStatus: String? = null,
    @SerialName("cycle_end")
    val cycleEnd: String? = null,
) {
    companion object {
        fun empty() = SignalSnapshot()
    }
}

@Serializable
data class ExecutionReportRequest(
    @SerialName("client_report_id")
    val clientReportId: String,
    val source: String,
    @SerialName("signal_id")
    val signalId: String,
    val symbol: String,
    @SerialName("period_seconds")
    val periodSeconds: Int,
    val direction: String,
    @SerialName("stake_usdt")
    val stakeUsdt: String,
    @SerialName("entry_price")
    val entryPrice: String,
    @SerialName("expiry_price")
    val expiryPrice: String,
    @SerialName("payout_ratio")
    val payoutRatio: String,
    @SerialName("gross_profit")
    val grossProfit: String,
    val result: String,
    @SerialName("automation_profile")
    val automationProfile: String?,
    @SerialName("automation_confidence")
    val automationConfidence: String?,
    @SerialName("action_trace_id")
    val actionTraceId: String?,
    @SerialName("reported_at")
    val reportedAt: String,
)

@Serializable
data class EventContractOrderRequest(
    @SerialName("client_order_id")
    val clientOrderId: String,
    val source: String,
    @SerialName("reservation_id")
    val reservationId: String? = null,
    @SerialName("signal_id")
    val signalId: String,
    val symbol: String,
    @SerialName("period_seconds")
    val periodSeconds: Int,
    val direction: String,
    @SerialName("stake_usdt")
    val stakeUsdt: String,
    @SerialName("payout_ratio")
    val payoutRatio: String? = null,
    @SerialName("expected_payout_usdt")
    val expectedPayoutUsdt: String? = null,
    @SerialName("expected_profit_usdt")
    val expectedProfitUsdt: String? = null,
    @SerialName("automation_profile")
    val automationProfile: String?,
    @SerialName("automation_confidence")
    val automationConfidence: String?,
    @SerialName("action_trace_id")
    val actionTraceId: String?,
    @SerialName("raw_audit")
    val rawAudit: Map<String, String>? = null,
    @SerialName("reported_at")
    val reportedAt: String,
)

@Serializable
data class AutomationPageStateRequest(
    @SerialName("page_ready")
    val pageReady: Boolean,
    @SerialName("screen_name")
    val screenName: String,
    val confidence: String,
    @SerialName("package_name")
    val packageName: String,
    @SerialName("amount_field_found")
    val amountFieldFound: Boolean,
    @SerialName("up_button_found")
    val upButtonFound: Boolean,
    @SerialName("down_button_found")
    val downButtonFound: Boolean,
    @SerialName("observed_at")
    val observedAt: String,
)
