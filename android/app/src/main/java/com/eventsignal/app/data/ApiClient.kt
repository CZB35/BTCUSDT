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

    fun clearAppSessionKey() {
        appSessionKey = null
    }

    fun register(walletAddress: String, password: String, deviceFingerprint: String): Result<AuthResponse> =
        auth("/v1/auth/register", walletAddress, password, deviceFingerprint)

    fun login(walletAddress: String, password: String, deviceFingerprint: String): Result<AuthResponse> =
        auth("/v1/auth/login", walletAddress, password, deviceFingerprint)

    fun getLatestSignal(): Result<SignalSnapshot> = runCatching {
        val request = signedRequest("/v1/signals/latest").get().build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("latest signal failed: ${response.code}")
            SignalJsonParser.parse(response.body?.string().orEmpty())
        }
    }

    fun websocketRequest(): Result<Request> = runCatching {
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
        val request = signedRequest("/v1/user-settings/automation").get().build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("automation settings failed: ${response.code}")
            json.decodeFromString<AutomationSettings>(response.body?.string().orEmpty())
        }
    }

    fun getExchangeStatus(): Result<ExchangeCredentialStatus> = runCatching {
        val request = signedRequest("/v1/exchange/binance").get().build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("exchange status failed: ${response.code}")
            json.decodeFromString<ExchangeCredentialStatus>(response.body?.string().orEmpty())
        }
    }

    fun saveExchangeCredentials(apiKey: String, apiSecret: String): Result<ExchangeCredentialResponse> = runCatching {
        val body = json.encodeToString(
            ExchangeCredentialRequest(
                apiKey = apiKey.trim(),
                apiSecret = apiSecret.trim(),
            ),
        ).toRequestBody(mediaType)
        val request = signedRequest("/v1/exchange/binance").post(body).build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("exchange bind failed: ${response.code}")
            json.decodeFromString<ExchangeCredentialResponse>(response.body?.string().orEmpty())
        }
    }

    fun saveAutomationSettings(settings: AutomationSettings): Result<AutomationSettings> = runCatching {
        val body = json.encodeToString(settings).toRequestBody(mediaType)
        val request = signedRequest("/v1/user-settings/automation").put(body).build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("save automation settings failed: ${response.code}")
            json.decodeFromString<AutomationSettings>(response.body?.string().orEmpty())
        }
    }

    fun submitAutomationPageState(state: AutomationPageStateRequest): Result<Unit> = runCatching {
        val body = json.encodeToString(state).toRequestBody(mediaType)
        val request = signedRequest("/v1/automation/page-state").post(body).build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("automation page state failed: ${response.code}")
        }
    }

    fun submitExecutionReport(report: ExecutionReportRequest): Result<Unit> = runCatching {
        val body = json.encodeToString(report).toRequestBody(mediaType)
        val request = signedRequest("/v1/execution-reports").post(body).build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("execution report failed: ${response.code}")
        }
    }

    fun submitEventContractOrder(order: EventContractOrderRequest): Result<Unit> = runCatching {
        val body = json.encodeToString(order).toRequestBody(mediaType)
        val request = signedRequest("/v1/event-contract-orders").post(body).build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("event contract order failed: ${response.code}")
        }
    }

    fun ackSignalDelivery(ack: SignalDeliveryAckRequest): Result<Unit> = runCatching {
        val body = json.encodeToString(ack).toRequestBody(mediaType)
        val request = signedRequest("/v1/signals/deliveries/ack").post(body).build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("signal delivery ack failed: ${response.code}")
        }
    }

    fun getAppRuntimeSummary(): Result<AppRuntimeSummary> = runCatching {
        val request = signedRequest("/v1/app/runtime-summary").get().build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("runtime summary failed: ${response.code}")
            json.decodeFromString<AppRuntimeSummary>(response.body?.string().orEmpty())
        }
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
        val key = appSessionKey?.trim().orEmpty()
        if (key.isBlank()) error("app session key is required")
        val timestamp = System.currentTimeMillis().toString()
        val nonce = UUID.randomUUID().toString().replace("-", "")
        return authedRequest(path)
            .header("X-ES-Timestamp", timestamp)
            .header("X-ES-Nonce", nonce)
            .header("X-ES-Signature", appSignature(key, timestamp, nonce))
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
                    .also {
                        accessToken = it.accessToken
                        appSessionKey = it.appSessionKey
                    }
            }
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
        raw.contains("AUTH_ACCOUNT_EXISTS", ignoreCase = true) -> "该钱包已注册，请直接登录"
        raw.contains("ACCOUNT_ALREADY_LOGGED_IN_ON_ANOTHER_DEVICE", ignoreCase = true) -> "该账号已在其他设备登录"
        raw.contains("CLIENT_TYPE_FORBIDDEN", ignoreCase = true) -> "当前账号类型不能登录 App"
        raw.contains("RATE_LIMITED", ignoreCase = true) -> "请求过快，请稍后再试"
        else -> null
    }
}

private fun appSignature(key: String, timestamp: String, nonce: String): String {
    val mac = Mac.getInstance("HmacSHA256")
    mac.init(SecretKeySpec(key.toByteArray(Charsets.UTF_8), "HmacSHA256"))
    val payload = listOf("android", timestamp, nonce).joinToString(":")
    return mac.doFinal(payload.toByteArray(Charsets.UTF_8)).joinToString("") { "%02x".format(it) }
}
