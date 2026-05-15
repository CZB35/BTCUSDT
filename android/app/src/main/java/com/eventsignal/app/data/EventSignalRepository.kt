package com.eventsignal.app.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import com.eventsignal.app.automation.AutomationStateStore
import com.eventsignal.app.domain.AutomationPageStateRequest
import com.eventsignal.app.domain.AutomationSettings
import com.eventsignal.app.domain.AppRuntimeSummary
import com.eventsignal.app.domain.AuthResponse
import com.eventsignal.app.domain.EventContractOrderRequest
import com.eventsignal.app.domain.ExchangeCredentialResponse
import com.eventsignal.app.domain.ExchangeCredentialStatus
import com.eventsignal.app.domain.ExecutionReportRequest
import com.eventsignal.app.domain.SignalDeliveryAckRequest
import com.eventsignal.app.domain.SignalSnapshot
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

class EventSignalRepository(context: Context) {
    private val api = ApiClient(context.applicationContext)
    private val stateStore = AutomationStateStore(context.applicationContext)
    private val prefs = context.applicationContext.getSharedPreferences("event_signal_exchange", Context.MODE_PRIVATE)

    companion object {
        private const val EXCHANGE_KEY_ALIAS = "event_signal_exchange_credentials"
        private const val EXCHANGE_CIPHER = "AES/GCM/NoPadding"
    }

    fun loadLatestSignal(): Result<SignalSnapshot> =
        api.getLatestSignal().onSuccess(stateStore::saveSignal)

    fun loadAutomationSettings(): Result<AutomationSettings> =
        api.getAutomationSettings().onSuccess(stateStore::saveSettings)

    fun loadExchangeStatus(): Result<ExchangeCredentialStatus> =
        api.getExchangeStatus()

    fun saveExchangeCredentials(apiKey: String, apiSecret: String): Result<ExchangeCredentialResponse> =
        api.saveExchangeCredentials(apiKey, apiSecret)
            .onSuccess {
                if (apiSecret.isNotBlank()) {
                    saveLocalExchangeCredentials(apiKey, apiSecret)
                } else {
                    saveLocalExchangeApiKey(apiKey)
                }
            }

    fun saveAutomationSettings(settings: AutomationSettings): Result<AutomationSettings> =
        api.saveAutomationSettings(settings).onSuccess(stateStore::saveSettings)

    fun submitAutomationPageState(state: AutomationPageStateRequest): Result<Unit> =
        api.submitAutomationPageState(state)

    fun submitExecutionReport(report: ExecutionReportRequest): Result<Unit> =
        api.submitExecutionReport(report)

    fun submitEventContractOrder(order: EventContractOrderRequest): Result<Unit> =
        api.submitEventContractOrder(order)

    fun releaseSignalReservation(reservationId: String, reason: String): Result<Unit> =
        api.releaseSignalReservation(reservationId, reason)

    fun ackSignalDelivery(ack: SignalDeliveryAckRequest): Result<Unit> =
        api.ackSignalDelivery(ack)

    fun loadAppRuntimeSummary(): Result<AppRuntimeSummary> =
        api.getAppRuntimeSummary().onSuccess(stateStore::saveRuntimeSummary)

    fun websocketRequest() =
        api.websocketRequest()

    fun register(walletAddress: String, password: String, deviceFingerprint: String): Result<AuthResponse> =
        api.register(walletAddress, password, deviceFingerprint)

    fun login(walletAddress: String, password: String, deviceFingerprint: String): Result<AuthResponse> =
        api.login(walletAddress, password, deviceFingerprint)

    fun refreshSession(): Result<AuthResponse> =
        api.refreshSession()

    fun dappUrl(): String = "${api.baseUrl}/dapp/?return_uri=eventsignal://wallet"

    fun accessToken(): String? = api.accessToken

    fun savedExchangeApiKey(): String =
        decryptLocalSecret(prefs.getString("binance_api_key", "").orEmpty())

    fun savedExchangeApiSecret(): String =
        decryptLocalSecret(prefs.getString("binance_api_secret", "").orEmpty())

    private fun saveLocalExchangeCredentials(apiKey: String, apiSecret: String) {
        prefs.edit()
            .putString("binance_api_key", encryptLocalSecret(apiKey.trim()))
            .putString("binance_api_secret", encryptLocalSecret(apiSecret.trim()))
            .apply()
    }

    private fun saveLocalExchangeApiKey(apiKey: String) {
        prefs.edit()
            .putString("binance_api_key", encryptLocalSecret(apiKey.trim()))
            .apply()
    }

    private fun encryptLocalSecret(value: String): String {
        if (value.isBlank()) return ""
        val cipher = Cipher.getInstance(EXCHANGE_CIPHER)
        cipher.init(Cipher.ENCRYPT_MODE, exchangeSecretKey())
        val iv = Base64.encodeToString(cipher.iv, Base64.NO_WRAP)
        val encrypted = Base64.encodeToString(cipher.doFinal(value.toByteArray(Charsets.UTF_8)), Base64.NO_WRAP)
        return "$iv:$encrypted"
    }

    private fun decryptLocalSecret(value: String): String {
        if (value.isBlank()) return ""
        if (!value.contains(":")) return value
        return runCatching {
            val parts = value.split(":", limit = 2)
            val iv = Base64.decode(parts[0], Base64.NO_WRAP)
            val encrypted = Base64.decode(parts[1], Base64.NO_WRAP)
            val cipher = Cipher.getInstance(EXCHANGE_CIPHER)
            cipher.init(Cipher.DECRYPT_MODE, exchangeSecretKey(), GCMParameterSpec(128, iv))
            String(cipher.doFinal(encrypted), Charsets.UTF_8)
        }.getOrDefault("")
    }

    private fun exchangeSecretKey(): SecretKey {
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (keyStore.getKey(EXCHANGE_KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        val spec = KeyGenParameterSpec.Builder(
            EXCHANGE_KEY_ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .build()
        generator.init(spec)
        return generator.generateKey()
    }

    fun logout() {
        api.clearAuth()
    }
}
