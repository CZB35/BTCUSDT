package com.eventsignal.app.data

import android.content.Context
import com.eventsignal.app.automation.AutomationStateStore
import com.eventsignal.app.domain.AutomationPageStateRequest
import com.eventsignal.app.domain.AutomationSettings
import com.eventsignal.app.domain.AuthResponse
import com.eventsignal.app.domain.EventContractOrderRequest
import com.eventsignal.app.domain.ExecutionReportRequest
import com.eventsignal.app.domain.SignalSnapshot

class EventSignalRepository(context: Context) {
    private val api = ApiClient(context.applicationContext)
    private val stateStore = AutomationStateStore(context.applicationContext)

    fun loadLatestSignal(): Result<SignalSnapshot> =
        api.getLatestSignal().onSuccess(stateStore::saveSignal)

    fun loadAutomationSettings(): Result<AutomationSettings> =
        api.getAutomationSettings().onSuccess(stateStore::saveSettings)

    fun saveAutomationSettings(settings: AutomationSettings): Result<AutomationSettings> =
        api.saveAutomationSettings(settings).onSuccess(stateStore::saveSettings)

    fun submitAutomationPageState(state: AutomationPageStateRequest): Result<Unit> =
        api.submitAutomationPageState(state)

    fun submitExecutionReport(report: ExecutionReportRequest): Result<Unit> =
        api.submitExecutionReport(report)

    fun submitEventContractOrder(order: EventContractOrderRequest): Result<Unit> =
        api.submitEventContractOrder(order)

    fun websocketRequest() =
        api.websocketRequest()

    fun register(walletAddress: String, password: String, deviceFingerprint: String): Result<AuthResponse> =
        api.register(walletAddress, password, deviceFingerprint)

    fun login(walletAddress: String, password: String, deviceFingerprint: String): Result<AuthResponse> =
        api.login(walletAddress, password, deviceFingerprint)

    fun dappUrl(): String = "${api.baseUrl}/dapp/?return_uri=eventsignal://wallet"

    fun accessToken(): String? = api.accessToken
}
