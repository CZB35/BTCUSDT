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
            automationEnabled = true,
            executionMode = "automatic",
            targetAppProfile = "binance_event_contract_v1",
            requireFinalConfirmation = false,
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
    @SerialName("client_type")
    val clientType: String = "android",
)

@Serializable
data class AuthResponse(
    @SerialName("access_token")
    val accessToken: String,
    @SerialName("refresh_token")
    val refreshToken: String,
    @SerialName("app_session_key")
    val appSessionKey: String,
    @SerialName("expires_in")
    val expiresIn: Long,
)

@Serializable
data class RefreshRequest(
    @SerialName("refresh_token")
    val refreshToken: String,
)

@Serializable
data class ExchangeCredentialStatus(
    val provider: String = "binance_futures",
    @SerialName("api_key")
    val apiKey: String? = null,
    val connected: Boolean = false,
    @SerialName("can_read_account")
    val canReadAccount: Boolean = false,
    @SerialName("verification_status")
    val verificationStatus: String = "missing",
    @SerialName("balance_available_usdt")
    val balanceAvailableUsdt: String? = null,
    @SerialName("balance_wallet_usdt")
    val balanceWalletUsdt: String? = null,
    @SerialName("account_match_status")
    val accountMatchStatus: String? = null,
    @SerialName("observed_balance_usdt")
    val observedBalanceUsdt: String? = null,
    @SerialName("account_balance_delta_usdt")
    val accountBalanceDeltaUsdt: String? = null,
    @SerialName("balance_updated_at")
    val balanceUpdatedAt: String? = null,
    @SerialName("balance_cache_fresh")
    val balanceCacheFresh: Boolean = false,
    @SerialName("last_verified_at")
    val lastVerifiedAt: String? = null,
    @SerialName("last_error")
    val lastError: String? = null,
)

@Serializable
data class ExchangeCredentialRequest(
    @SerialName("api_key")
    val apiKey: String,
    @SerialName("api_secret")
    val apiSecret: String,
)

@Serializable
data class ExchangeCredentialResponse(
    val provider: String,
    val connected: Boolean,
    @SerialName("can_read_account")
    val canReadAccount: Boolean,
    @SerialName("verification_status")
    val verificationStatus: String,
    @SerialName("balance_available_usdt")
    val balanceAvailableUsdt: String,
    @SerialName("balance_wallet_usdt")
    val balanceWalletUsdt: String,
    @SerialName("last_verified_at")
    val lastVerifiedAt: String? = null,
    val message: String,
)

@Serializable
data class SignalSnapshot(
    val id: String? = null,
    @SerialName("delivery_id")
    val deliveryId: String? = null,
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
    @SerialName("account_match_status")
    val accountMatchStatus: String? = null,
    @SerialName("observed_balance_usdt")
    val observedBalanceUsdt: String? = null,
    @SerialName("account_balance_delta_usdt")
    val accountBalanceDeltaUsdt: String? = null,
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
data class ReleaseSignalReservationRequest(
    @SerialName("reservation_id")
    val reservationId: String,
    val reason: String? = null,
)

@Serializable
data class SignalDeliveryAckRequest(
    @SerialName("delivery_id")
    val deliveryId: String,
    @SerialName("signal_id")
    val signalId: String,
    @SerialName("reservation_id")
    val reservationId: String? = null,
    @SerialName("received_at")
    val receivedAt: String,
    @SerialName("app_version")
    val appVersion: String? = null,
    @SerialName("page_state")
    val pageState: Map<String, String>? = null,
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
    @SerialName("observed_balance_usdt")
    val observedBalanceUsdt: String? = null,
    @SerialName("observed_at")
    val observedAt: String,
)

@Serializable
data class AppRuntimeSummary(
    @SerialName("orders_total")
    val ordersTotal: Int = 0,
    val wins: Int = 0,
    val losses: Int = 0,
    @SerialName("win_rate")
    val winRate: String = "0",
    @SerialName("pnl_usdt")
    val pnlUsdt: String = "0",
    @SerialName("pending_settlements")
    val pendingSettlements: Int = 0,
)
