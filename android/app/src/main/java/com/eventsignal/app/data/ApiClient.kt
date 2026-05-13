package com.eventsignal.app.data

import android.content.Context
import com.eventsignal.app.domain.AuthRequest
import com.eventsignal.app.domain.AuthResponse
import com.eventsignal.app.domain.AutomationSettings
import com.eventsignal.app.domain.AutomationPageStateRequest
import com.eventsignal.app.domain.EventContractOrderRequest
import com.eventsignal.app.domain.ExecutionReportRequest
import com.eventsignal.app.domain.SignalSnapshot
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
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

    var baseUrl: String
        get() = prefs.getString("base_url", "http://10.0.2.2:8787") ?: "http://10.0.2.2:8787"
        set(value) = prefs.edit().putString("base_url", value.trimEnd('/')).apply()

    var accessToken: String?
        get() = prefs.getString("access_token", null)
        set(value) = prefs.edit().putString("access_token", value).apply()

    fun saveConnection(baseUrl: String, accessToken: String) {
        prefs.edit()
            .putString("base_url", baseUrl.trimEnd('/'))
            .putString("access_token", accessToken.trim())
            .apply()
    }

    fun register(walletAddress: String, password: String, deviceFingerprint: String): Result<AuthResponse> =
        auth("/v1/auth/register", walletAddress, password, deviceFingerprint)

    fun login(walletAddress: String, password: String, deviceFingerprint: String): Result<AuthResponse> =
        auth("/v1/auth/login", walletAddress, password, deviceFingerprint)

    fun getLatestSignal(): Result<SignalSnapshot> = runCatching {
        val request = authedRequest("/v1/signals/latest").get().build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("latest signal failed: ${response.code}")
            SignalJsonParser.parse(response.body?.string().orEmpty())
        }
    }

    fun websocketRequest(): Result<Request> = runCatching {
        val token = accessToken?.trim().orEmpty()
        if (token.isBlank()) error("access token is required")
        val base = baseUrl.trimEnd('/')
        val wsBase = when {
            base.startsWith("https://", ignoreCase = true) -> "wss://" + base.removePrefix("https://")
            base.startsWith("http://", ignoreCase = true) -> "ws://" + base.removePrefix("http://")
            else -> "ws://$base"
        }
        Request.Builder()
            .url("$wsBase/v1/ws?access_token=$token")
            .build()
    }

    fun getAutomationSettings(): Result<AutomationSettings> = runCatching {
        val request = authedRequest("/v1/user-settings/automation").get().build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("automation settings failed: ${response.code}")
            json.decodeFromString<AutomationSettings>(response.body?.string().orEmpty())
        }
    }

    fun saveAutomationSettings(settings: AutomationSettings): Result<AutomationSettings> = runCatching {
        val body = json.encodeToString(settings).toRequestBody(mediaType)
        val request = authedRequest("/v1/user-settings/automation").put(body).build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("save automation settings failed: ${response.code}")
            json.decodeFromString<AutomationSettings>(response.body?.string().orEmpty())
        }
    }

    fun submitAutomationPageState(state: AutomationPageStateRequest): Result<Unit> = runCatching {
        val body = json.encodeToString(state).toRequestBody(mediaType)
        val request = authedRequest("/v1/automation/page-state").post(body).build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("automation page state failed: ${response.code}")
        }
    }

    fun submitExecutionReport(report: ExecutionReportRequest): Result<Unit> = runCatching {
        val body = json.encodeToString(report).toRequestBody(mediaType)
        val request = authedRequest("/v1/execution-reports").post(body).build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("execution report failed: ${response.code}")
        }
    }

    fun submitEventContractOrder(order: EventContractOrderRequest): Result<Unit> = runCatching {
        val body = json.encodeToString(order).toRequestBody(mediaType)
        val request = authedRequest("/v1/event-contract-orders").post(body).build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("event contract order failed: ${response.code}")
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

    private fun auth(path: String, walletAddress: String, password: String, deviceFingerprint: String): Result<AuthResponse> =
        runCatching {
            if (walletAddress.isBlank() || password.isBlank()) error("wallet address and password are required")
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
                if (!response.isSuccessful) error("auth failed: ${response.code}")
                json.decodeFromString<AuthResponse>(response.body?.string().orEmpty())
                    .also { accessToken = it.accessToken }
            }
        }
}
