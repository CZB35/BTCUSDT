package com.eventsignal.app.data

import android.content.Context
import com.eventsignal.app.BuildConfig
import com.eventsignal.app.domain.AppRuntimeSummary
import com.eventsignal.app.domain.AuthRequest
import com.eventsignal.app.domain.AuthResponse
import com.eventsignal.app.domain.AutomationSettings
import com.eventsignal.app.domain.AutomationPageStateRequest
import com.eventsignal.app.domain.EventContractOrderRequest
import com.eventsignal.app.domain.ExchangeCredentialRequest
import com.eventsignal.app.domain.ExchangeCredentialResponse
import com.eventsignal.app.domain.ExchangeCredentialStatus
import com.eventsignal.app.domain.ExecutionReportRequest
import com.eventsignal.app.domain.RefreshRequest
import com.eventsignal.app.domain.ReleaseSignalReservationRequest
import com.eventsignal.app.domain.SignalDeliveryAckRequest
import com.eventsignal.app.domain.SignalSnapshot
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec
import java.util.UUID
import java.util.concurrent.TimeUnit

class ApiClient(context: Context) {
    private val prefs = context.getSharedPreferences("event_signal_api", Context.MODE_PRIVATE)
    private val http = OkHttpClient.Builder()
        .connectTimeout(3, TimeUnit.SECONDS)
        .readTimeout(5, TimeUnit.SECONDS)
        .writeTimeout(5, TimeUnit.SECONDS)
        .callTimeout(8, TimeUnit.SECONDS)
        .build()
    private val json = Json {
        ignoreUnknownKeys = true
    }
    private val mediaType = "application/json".toMediaType()

    val baseUrl: String = BuildConfig.API_BASE_URL.trimEnd('/')

    var accessToken: String?
        get() = prefs.getString("access_token", null)
        set(value) {
            val editor = prefs.edit()
            if (value.isNullOrBlank()) {
                editor.remove("access_token")
            } else {
                editor.putString("access_token", value)
            }
            editor.apply()
        }

    private var accessTokenExpiresAtMs: Long
        get() = prefs.getLong("access_token_expires_at_ms", 0L)
        set(value) {
            val editor = prefs.edit()
            if (value <= 0L) {
                editor.remove("access_token_expires_at_ms")
            } else {
                editor.putLong("access_token_expires_at_ms", value)
            }
            editor.apply()
        }

    var appSessionKey: String?
        get() = prefs.getString("app_session_key", null)
        private set(value) {
            val editor = prefs.edit()
            if (value.isNullOrBlank()) {
                editor.remove("app_session_key")
            } else {
                editor.putString("app_session_key", value)
            }
            editor.apply()
        }

    private var refreshToken: String?
        get() = prefs.getString("refresh_token", null)
        set(value) {
            val editor = prefs.edit()
            if (value.isNullOrBlank()) {
                editor.remove("refresh_token")
            } else {
                editor.putString("refresh_token", value)
            }
            editor.apply()
        }

    fun clearAuth() {
        accessToken = null
        refreshToken = null
        appSessionKey = null
        accessTokenExpiresAtMs = 0L
    }

    fun register(walletAddress: String, password: String, deviceFingerprint: String): Result<AuthResponse> =
        auth("/v1/auth/register", walletAddress, password, deviceFingerprint)

    fun login(walletAddress: String, password: String, deviceFingerprint: String): Result<AuthResponse> =
        auth("/v1/auth/login", walletAddress, password, deviceFingerprint)

    fun refreshSession(): Result<AuthResponse> = runCatching {
        val token = refreshToken?.trim().orEmpty()
        if (token.isBlank()) error("refresh token is required")
        val body = json.encodeToString(RefreshRequest(token)).toRequestBody(mediaType)
        val request = Request.Builder()
            .url("${baseUrl.trimEnd('/')}/v1/auth/refresh")
            .post(body)
            .build()
        http.newCall(request).execute().use { response ->
            val responseBody = response.body?.string().orEmpty()
            if (!response.isSuccessful) {
                if (isAuthInvalid(responseBody)) {
                    clearAuth()
                }
                error(friendlyError(responseBody) ?: "刷新登录失败：HTTP ${response.code}")
            }
            json.decodeFromString<AuthResponse>(responseBody).also(::saveAuth)
        }
    }

    fun getLatestSignal(): Result<SignalSnapshot> = runCatching {
        SignalJsonParser.parse(
            executeSigned("/v1/signals/latest", "latest signal failed") {
                get().build()
            },
        )
    }

    fun websocketRequest(): Result<Request> = runCatching {
        ensureFreshSessionForRequest()
        val token = accessToken?.trim().orEmpty()
        val key = appSessionKey?.trim().orEmpty()
        if (token.isBlank()) error("access token is required")
        if (key.isBlank()) error("app session key is required")
        val timestamp = System.currentTimeMillis().toString()
        val nonce = UUID.randomUUID().toString().replace("-", "")
        val signature = appSignature(key, timestamp, nonce)
        val base = baseUrl.trimEnd('/')
        val wsBase = when {
            base.startsWith("https://", ignoreCase = true) -> "wss://" + base.removePrefix("https://")
            base.startsWith("http://", ignoreCase = true) -> "ws://" + base.removePrefix("http://")
            else -> "ws://$base"
        }
        Request.Builder()
            .url("$wsBase/v1/ws?access_token=$token")
            .header("X-ES-Timestamp", timestamp)
            .header("X-ES-Nonce", nonce)
            .header("X-ES-Signature", signature)
            .build()
    }

    fun getAutomationSettings(): Result<AutomationSettings> = runCatching {
        json.decodeFromString<AutomationSettings>(
            executeSigned("/v1/user-settings/automation", "automation settings failed") {
                get().build()
            },
        )
    }

    fun getExchangeStatus(): Result<ExchangeCredentialStatus> = runCatching {
        json.decodeFromString<ExchangeCredentialStatus>(
            executeSigned("/v1/exchange/binance", "exchange status failed") {
                get().build()
            },
        )
    }

    fun saveExchangeCredentials(apiKey: String, apiSecret: String): Result<ExchangeCredentialResponse> = runCatching {
        val body = json.encodeToString(
            ExchangeCredentialRequest(
                apiKey = apiKey.trim(),
                apiSecret = apiSecret.trim(),
            ),
        ).toRequestBody(mediaType)
        json.decodeFromString<ExchangeCredentialResponse>(
            executeSigned("/v1/exchange/binance", "exchange bind failed") {
                post(body).build()
            },
        )
    }

    fun saveAutomationSettings(settings: AutomationSettings): Result<AutomationSettings> = runCatching {
        val body = json.encodeToString(settings).toRequestBody(mediaType)
        json.decodeFromString<AutomationSettings>(
            executeSigned("/v1/user-settings/automation", "save automation settings failed") {
                put(body).build()
            },
        )
    }

    fun submitAutomationPageState(state: AutomationPageStateRequest): Result<Unit> = runCatching {
        val body = json.encodeToString(state).toRequestBody(mediaType)
        executeSigned("/v1/automation/page-state", "automation page state failed") {
            post(body).build()
        }
    }

    fun submitExecutionReport(report: ExecutionReportRequest): Result<Unit> = runCatching {
        val body = json.encodeToString(report).toRequestBody(mediaType)
        executeSigned("/v1/execution-reports", "execution report failed") {
            post(body).build()
        }
    }

    fun submitEventContractOrder(order: EventContractOrderRequest): Result<Unit> = runCatching {
        val body = json.encodeToString(order).toRequestBody(mediaType)
        executeSigned("/v1/event-contract-orders", "event contract order failed") {
            post(body).build()
        }
    }

    fun releaseSignalReservation(reservationId: String, reason: String): Result<Unit> = runCatching {
        val body = json.encodeToString(
            ReleaseSignalReservationRequest(
                reservationId = reservationId,
                reason = reason,
            ),
        ).toRequestBody(mediaType)
        executeSigned("/v1/signal-reservations/release", "release reservation failed") {
            post(body).build()
        }
    }

    fun ackSignalDelivery(ack: SignalDeliveryAckRequest): Result<Unit> = runCatching {
        val body = json.encodeToString(ack).toRequestBody(mediaType)
        executeSigned("/v1/signals/deliveries/ack", "signal delivery ack failed") {
            post(body).build()
        }
    }

    fun getAppRuntimeSummary(): Result<AppRuntimeSummary> = runCatching {
        json.decodeFromString<AppRuntimeSummary>(
            executeSigned("/v1/app/runtime-summary", "runtime summary failed") {
                get().build()
            },
        )
    }

    private fun authedRequest(path: String): Request.Builder {
        val builder = Request.Builder()
            .url("${baseUrl.trimEnd('/')}$path")
        accessToken?.takeIf { it.isNotBlank() }?.let { token ->
            builder.header("Authorization", "Bearer $token")
        }
        return builder
    }

    private fun signedRequest(path: String): Request.Builder {
        ensureFreshSessionForRequest()
        val key = appSessionKey?.trim().orEmpty()
        if (key.isBlank()) error("app session key is required")
        val timestamp = System.currentTimeMillis().toString()
        val nonce = UUID.randomUUID().toString().replace("-", "")
        return authedRequest(path)
            .header("X-ES-Timestamp", timestamp)
            .header("X-ES-Nonce", nonce)
            .header("X-ES-Signature", appSignature(key, timestamp, nonce))
    }

    private fun executeSigned(
        path: String,
        errorPrefix: String,
        create: Request.Builder.() -> Request,
    ): String {
        var retried = false
        while (true) {
            var shouldRetry = false
            var output: String? = null
            val request = signedRequest(path).create()
            http.newCall(request).execute().use { response ->
                val responseBody = response.body?.string().orEmpty()
                if (response.code == 401 && !retried && refreshSessionForRetry()) {
                    shouldRetry = true
                } else {
                    if (!response.isSuccessful) {
                        error(friendlyError(responseBody) ?: "$errorPrefix: HTTP ${response.code}")
                    }
                    output = responseBody
                }
            }
            if (shouldRetry) {
                retried = true
                continue
            }
            return output.orEmpty()
        }
    }

    private fun refreshSessionForRetry(): Boolean =
        refreshSession().isSuccess

    private fun ensureFreshSessionForRequest() {
        if (sessionNeedsRefresh() && !refreshToken.isNullOrBlank()) {
            refreshSessionForRetry()
        }
    }

    private fun sessionNeedsRefresh(): Boolean {
        if (accessToken.isNullOrBlank() || appSessionKey.isNullOrBlank()) {
            return true
        }
        val expiresAt = accessTokenExpiresAtMs
        return expiresAt <= 0L || System.currentTimeMillis() >= expiresAt - TOKEN_REFRESH_MARGIN_MS
    }

    private fun auth(path: String, walletAddress: String, password: String, deviceFingerprint: String): Result<AuthResponse> =
        runCatching {
            if (walletAddress.isBlank()) error("请输入钱包地址")
            if (password.length < 6) error("密码至少 6 位")
            val body = json.encodeToString(
                AuthRequest(
                    walletAddress = walletAddress.trim(),
                    password = password,
                    deviceFingerprint = deviceFingerprint,
                ),
            ).toRequestBody(mediaType)
            val request = Request.Builder()
                .url("${baseUrl.trimEnd('/')}$path")
                .post(body)
                .build()
            http.newCall(request).execute().use { response ->
                if (!response.isSuccessful) error(httpError("登录失败", response))
                json.decodeFromString<AuthResponse>(response.body?.string().orEmpty())
                    .also(::saveAuth)
            }
        }

    private fun saveAuth(response: AuthResponse) {
        accessToken = response.accessToken
        refreshToken = response.refreshToken
        appSessionKey = response.appSessionKey
        val ttlMs = response.expiresIn.coerceIn(0L, MAX_AUTH_TTL_SECONDS) * 1000L
        accessTokenExpiresAtMs = if (ttlMs > 0L) System.currentTimeMillis() + ttlMs else 0L
    }

    private fun httpError(prefix: String, response: okhttp3.Response): String {
        val raw = response.body?.string().orEmpty()
        val friendly = friendlyError(raw)
        return friendly ?: "$prefix：HTTP ${response.code}"
    }

    private fun friendlyError(raw: String): String? = when {
        raw.contains("AUTH_PASSWORD_WEAK", ignoreCase = true) -> "密码至少 6 位"
        raw.contains("AUTH_INVALID_CREDENTIALS", ignoreCase = true) -> "钱包地址或密码错误"
        raw.contains("AUTH_SESSION_REVOKED", ignoreCase = true) -> "账号会话已失效，请重新登录"
        raw.contains("AUTH_TOKEN_EXPIRED", ignoreCase = true) -> "登录已过期，请重新登录"
        raw.contains("AUTH_INVALID_REFRESH_TOKEN", ignoreCase = true) -> "登录已过期，请重新登录"
        raw.contains("AUTH_ACCOUNT_EXISTS", ignoreCase = true) -> "该钱包已注册，请直接登录"
        raw.contains("ACCOUNT_ALREADY_LOGGED_IN_ON_ANOTHER_DEVICE", ignoreCase = true) -> "该账号已在其他设备登录"
        raw.contains("CLIENT_TYPE_FORBIDDEN", ignoreCase = true) -> "当前账号类型不能登录 App"
        raw.contains("RATE_LIMITED", ignoreCase = true) -> "请求过快，请稍后再试"
        else -> null
    }

    private fun isAuthInvalid(raw: String): Boolean =
        raw.contains("AUTH_INVALID_REFRESH_TOKEN", ignoreCase = true) ||
            raw.contains("AUTH_SESSION_REVOKED", ignoreCase = true) ||
            raw.contains("DEVICE_SESSION_REPLACED", ignoreCase = true)
}

private fun appSignature(key: String, timestamp: String, nonce: String): String {
    val mac = Mac.getInstance("HmacSHA256")
    mac.init(SecretKeySpec(key.toByteArray(Charsets.UTF_8), "HmacSHA256"))
    val payload = listOf("android", timestamp, nonce).joinToString(":")
    return mac.doFinal(payload.toByteArray(Charsets.UTF_8)).joinToString("") { "%02x".format(it) }
}

private const val TOKEN_REFRESH_MARGIN_MS = 60_000L
private const val MAX_AUTH_TTL_SECONDS = 30L * 24L * 60L * 60L
