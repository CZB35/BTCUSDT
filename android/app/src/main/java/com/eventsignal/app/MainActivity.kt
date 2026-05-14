package com.eventsignal.app

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import com.eventsignal.app.automation.AutomationDebugState
import com.eventsignal.app.automation.AutomationStateStore
import com.eventsignal.app.data.EventSignalRepository
import com.eventsignal.app.domain.AutomationSettings
import com.eventsignal.app.domain.ExchangeCredentialStatus
import com.eventsignal.app.domain.SignalSnapshot
import com.eventsignal.app.service.AutomationForegroundService
import com.eventsignal.app.service.AutomationRuntime
import com.eventsignal.app.service.FloatingSignalService
import com.eventsignal.app.service.SignalForegroundService
import com.eventsignal.app.ui.BinanceYellow
import com.eventsignal.app.ui.BodyOnDark
import com.eventsignal.app.ui.CanvasDark
import com.eventsignal.app.ui.EventSignalTheme
import com.eventsignal.app.ui.HairlineDark
import com.eventsignal.app.ui.MutedText
import com.eventsignal.app.ui.SurfaceElevatedDark
import com.eventsignal.app.ui.TradingDown
import com.eventsignal.app.ui.TradingUp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlin.math.roundToInt

class MainActivity : ComponentActivity() {
    private val notificationPermissionLauncher =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestNotificationPermissionIfNeeded()
        val repository = EventSignalRepository(applicationContext)
        setContent {
            EventSignalTheme {
                MainScreen(repository = repository)
            }
        }
    }

    private fun requestNotificationPermissionIfNeeded() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val granted = ContextCompat.checkSelfPermission(
            this,
            Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
        if (!granted) {
            notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun MainScreen(repository: EventSignalRepository) {
    val context = LocalContext.current
    val stateStore = remember { AutomationStateStore(context.applicationContext) }
    val scope = rememberCoroutineScope()
    var signal by remember { mutableStateOf(SignalSnapshot.empty()) }
    var automationSettings by remember { mutableStateOf(AutomationSettings.default()) }
    var debugState by remember { mutableStateOf(AutomationDebugState()) }
    var connectionState by remember { mutableStateOf(stateStore.loadConnectionState()) }
    var lastSocketStatus by remember { mutableStateOf(stateStore.loadLastStatus()) }
    var automationRunning by remember { mutableStateOf(false) }
    var status by remember { mutableStateOf("就绪") }
    var accessToken by remember { mutableStateOf(repository.accessToken().orEmpty()) }
    var walletAddress by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var exchangeStatus by remember { mutableStateOf(ExchangeCredentialStatus()) }
    var binanceApiKey by remember { mutableStateOf("") }
    var binanceApiSecret by remember { mutableStateOf("") }
    val walletDeepLink = (context as? Activity)?.intent?.data?.getQueryParameter("address").orEmpty()
    val isLoggedIn = accessToken.isNotBlank()
    val tradingAccountReady = exchangeReady(exchangeStatus)

    suspend fun refresh() {
        val (nextSignal, nextSettings, nextExchangeStatus) = withContext(Dispatchers.IO) {
            Triple(
                repository.loadLatestSignal().getOrDefault(SignalSnapshot.empty()),
                repository.loadAutomationSettings().getOrDefault(stateStore.loadSettings()),
                if (repository.accessToken().isNullOrBlank()) {
                    ExchangeCredentialStatus()
                } else {
                    repository.loadExchangeStatus().getOrDefault(ExchangeCredentialStatus())
                },
            )
        }
        signal = nextSignal
        automationSettings = nextSettings
        exchangeStatus = nextExchangeStatus
        debugState = stateStore.loadDebugState()
        connectionState = stateStore.loadConnectionState()
        lastSocketStatus = stateStore.loadLastStatus()
    }

    fun persistAutomationSettings(next: AutomationSettings) {
        automationSettings = next
        stateStore.saveSettings(next)
        scope.launch(Dispatchers.IO) {
            repository.saveAutomationSettings(next)
        }
    }

    LaunchedEffect(Unit) {
        automationSettings = stateStore.loadSettings()
        if (walletDeepLink.startsWith("0x", ignoreCase = true)) {
            walletAddress = walletDeepLink
            status = "已从钱包回填地址"
        }
        refresh()
    }

    LaunchedEffect(Unit) {
        while (isActive) {
            debugState = stateStore.loadDebugState()
            connectionState = stateStore.loadConnectionState()
            lastSocketStatus = stateStore.loadLastStatus()
            delay(500)
        }
    }

    Scaffold(
        containerColor = CanvasDark,
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text("事件信号", fontWeight = FontWeight.SemiBold)
                        Text("BTCUSDT 事件合约自动化", style = MaterialTheme.typography.labelSmall, color = MutedText)
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = CanvasDark,
                    titleContentColor = BodyOnDark,
                    actionIconContentColor = BodyOnDark,
                ),
                actions = {
                    IconButton(onClick = { context.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)) }) {
                        Icon(Icons.Default.Settings, contentDescription = "自动化权限")
                    }
                    IconButton(onClick = {
                        status = "刷新中"
                        scope.launch {
                            refresh()
                            status = "就绪"
                        }
                    }) {
                        Icon(Icons.Default.Refresh, contentDescription = "刷新")
                    }
                },
            )
        },
    ) { padding ->
        Surface(modifier = Modifier.fillMaxSize().padding(padding), color = CanvasDark) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                AuthCard(
                    accessToken = accessToken,
                    walletAddress = walletAddress,
                    password = password,
                    status = status,
                    apiBaseUrl = repository.apiBaseUrl(),
                    onWalletAddressChange = { walletAddress = it },
                    onPasswordChange = { password = it },
                    onOpenDapp = {
                        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(repository.dappUrl())))
                        status = "已打开充值 DApp"
                    },
                    onLogin = {
                        val device = deviceFingerprint(context)
                        scope.launch {
                            status = "登录中"
                            val result = withContext(Dispatchers.IO) {
                                repository.login(walletAddress, password, device)
                            }
                            result.onSuccess {
                                accessToken = it.accessToken
                                status = "登录成功"
                                refresh()
                            }.onFailure {
                                status = "登录失败：${it.message ?: it.javaClass.simpleName}"
                            }
                        }
                    },
                    onRegister = {
                        val device = deviceFingerprint(context)
                        scope.launch {
                            status = "注册中"
                            val result = withContext(Dispatchers.IO) {
                                repository.register(walletAddress, password, device)
                            }
                            result.onSuccess {
                                accessToken = it.accessToken
                                status = "注册并登录成功"
                                refresh()
                            }.onFailure {
                                status = "注册失败：${it.message ?: it.javaClass.simpleName}"
                            }
                        }
                    },
                    onLogout = {
                        repository.logout()
                        context.stopService(Intent(context, SignalForegroundService::class.java))
                        context.stopService(Intent(context, AutomationForegroundService::class.java))
                        context.stopService(Intent(context, FloatingSignalService::class.java))
                        accessToken = ""
                        automationRunning = false
                        exchangeStatus = ExchangeCredentialStatus()
                        signal = SignalSnapshot.empty()
                        status = "已退出登录"
                    },
                )
                if (isLoggedIn) {
                    ExchangeCard(
                        status = exchangeStatus,
                        apiKey = binanceApiKey,
                        apiSecret = binanceApiSecret,
                        onApiKeyChange = { binanceApiKey = it },
                        onApiSecretChange = { binanceApiSecret = it },
                        onBind = {
                            scope.launch {
                                status = "正在验证币安只读 API"
                                val result = withContext(Dispatchers.IO) {
                                    repository.saveExchangeCredentials(binanceApiKey, binanceApiSecret)
                                }
                                result.onSuccess {
                                exchangeStatus = exchangeStatus.copy(
                                    connected = it.connected,
                                    canReadAccount = it.canReadAccount,
                                    verificationStatus = it.verificationStatus,
                                    balanceAvailableUsdt = it.balanceAvailableUsdt,
                                    balanceWalletUsdt = it.balanceWalletUsdt,
                                    lastVerifiedAt = it.lastVerifiedAt,
                                    lastError = null,
                                )
                                    binanceApiSecret = ""
                                    status = if (exchangeReady(exchangeStatus)) "交易账号已验证" else "交易账号验证失败"
                                }.onFailure {
                                    status = "绑定失败：${it.message ?: it.javaClass.simpleName}"
                                }
                            }
                        },
                    )
                    SignalCard(signal)
                    AutomationCard(
                        settings = automationSettings,
                        debugState = debugState,
                        connectionState = connectionState,
                        lastSocketStatus = lastSocketStatus,
                        running = automationRunning,
                        status = status,
                        tradingAccountReady = tradingAccountReady,
                        onToggle = { enabled ->
                            if (enabled && !tradingAccountReady) {
                                status = "请先绑定并验证交易账号"
                                return@AutomationCard
                            }
                            persistAutomationSettings(automationSettings.copy(automationEnabled = enabled))
                            status = if (enabled) "自动化已开启" else "自动化已关闭"
                        },
                        onModeChange = { mode ->
                            if (mode != "manual" && !tradingAccountReady) {
                                status = "请先绑定并验证交易账号"
                                return@AutomationCard
                            }
                            persistAutomationSettings(automationSettings.copy(executionMode = mode))
                            status = "执行模式已切换为${modeLabel(mode)}"
                        },
                        onStart = {
                            if (accessToken.isBlank()) {
                                status = "请先登录账号"
                                return@AutomationCard
                            }
                            if (!tradingAccountReady) {
                                status = "请先绑定并验证交易账号"
                                return@AutomationCard
                            }
                            if (!Settings.canDrawOverlays(context)) {
                                val intent = Intent(
                                    Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                    Uri.parse("package:${context.packageName}"),
                                )
                                context.startActivity(intent)
                                status = "请先开启悬浮窗权限"
                                return@AutomationCard
                            }
                            context.startForegroundService(Intent(context, SignalForegroundService::class.java))
                            context.startForegroundService(Intent(context, AutomationForegroundService::class.java))
                            context.startForegroundService(Intent(context, FloatingSignalService::class.java))
                            automationRunning = true
                            status = "自动化服务已启动"
                        },
                        onCalibrate = {
                            AutomationRuntime.requestCalibration()
                            status = "已请求识别当前页面"
                        },
                        onStop = {
                            context.stopService(Intent(context, SignalForegroundService::class.java))
                            context.stopService(Intent(context, AutomationForegroundService::class.java))
                            context.stopService(Intent(context, FloatingSignalService::class.java))
                            automationRunning = false
                            status = "自动化已停止"
                        },
                        onAccessibilityPermission = {
                            context.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
                        },
                        onOverlayPermission = {
                            val intent = Intent(
                                Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                Uri.parse("package:${context.packageName}"),
                            )
                            context.startActivity(intent)
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun AuthCard(
    accessToken: String,
    walletAddress: String,
    password: String,
    status: String,
    apiBaseUrl: String,
    onWalletAddressChange: (String) -> Unit,
    onPasswordChange: (String) -> Unit,
    onOpenDapp: () -> Unit,
    onLogin: () -> Unit,
    onRegister: () -> Unit,
    onLogout: () -> Unit,
) {
    val loggedIn = accessToken.isNotBlank()
    val walletReady = walletAddress.trim().startsWith("0x", ignoreCase = true) && walletAddress.trim().length >= 42
    val passwordReady = password.length >= 6
    val canSubmit = walletReady && passwordReady
    AppCard {
        SectionHeader(
            title = if (loggedIn) "账号已登录" else "账号登录",
            subtitle = if (loggedIn) "自动化服务启动后会接收后端 WebSocket 信号" else "连接钱包可自动填地址，未注册的钱包先注册",
        )
        OutlinedTextField(
            value = walletAddress,
            onValueChange = onWalletAddressChange,
            label = { Text("钱包地址") },
            singleLine = true,
            enabled = !loggedIn,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = password,
            onValueChange = onPasswordChange,
            label = { Text("密码") },
            singleLine = true,
            enabled = !loggedIn,
            visualTransformation = PasswordVisualTransformation(),
            modifier = Modifier.fillMaxWidth(),
        )
        if (loggedIn) {
            StatusChip(
                label = "登录状态：已连接",
                active = true,
                modifier = Modifier.fillMaxWidth(),
            )
            SecondaryButton(text = "退出登录", modifier = Modifier.fillMaxWidth(), onClick = onLogout)
        } else {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                PrimaryButton(text = "登录", modifier = Modifier.weight(1f), enabled = canSubmit, onClick = onLogin)
                SecondaryButton(text = "注册", modifier = Modifier.weight(1f), enabled = canSubmit, onClick = onRegister)
            }
            SecondaryButton(text = "连接钱包 / 注册充值", modifier = Modifier.fillMaxWidth(), onClick = onOpenDapp)
            StatusChip(
                label = if (canSubmit) "账号信息：可提交" else "账号信息：请输入钱包地址和 6 位以上密码",
                active = canSubmit,
                modifier = Modifier.fillMaxWidth(),
            )
        }
        Text("后端：$apiBaseUrl", style = MaterialTheme.typography.bodySmall, color = MutedText)
        Text(status, style = MaterialTheme.typography.bodySmall, color = MutedText)
    }
}

@Composable
private fun ExchangeCard(
    status: ExchangeCredentialStatus,
    apiKey: String,
    apiSecret: String,
    onApiKeyChange: (String) -> Unit,
    onApiSecretChange: (String) -> Unit,
    onBind: () -> Unit,
) {
    AppCard {
        SectionHeader(
            title = "币安只读 API",
            subtitle = "用于校验交易账号和同步事件合约结果，不需要开通交易权限",
        )
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            StatusChip(
                "连接：${exchangeConnectionLabel(status)}",
                status.connected && status.canReadAccount,
                Modifier.weight(1f),
            )
            StatusChip(
                "读取权限：${if (status.canReadAccount) "正常" else "未验证"}",
                status.connected && status.canReadAccount,
                Modifier.weight(1f),
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            StatusChip(
                "账号校验：${accountGateLabel(status.accountMatchStatus)}",
                accountGateReady(status.accountMatchStatus),
                Modifier.weight(1f),
            )
            StatusChip(
                "自动下单：${if (accountGateReady(status.accountMatchStatus)) "可用" else "不可用"}",
                accountGateReady(status.accountMatchStatus),
                Modifier.weight(1f),
            )
        }
        OutlinedTextField(
            value = apiKey,
            onValueChange = onApiKeyChange,
            label = { Text("API Key") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = apiSecret,
            onValueChange = onApiSecretChange,
            label = { Text("API Secret") },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            modifier = Modifier.fillMaxWidth(),
        )
        PrimaryButton(
            text = "绑定并验证",
            modifier = Modifier.fillMaxWidth(),
            enabled = apiKey.isNotBlank() && apiSecret.isNotBlank(),
            onClick = onBind,
        )
        Text(
            "账号校验通过后才会自动执行信号；校验失败时不会自动下单。",
            style = MaterialTheme.typography.bodySmall,
            color = MutedText,
        )
        status.lastError?.takeIf { it.isNotBlank() }?.let {
            Text("最近错误：$it", style = MaterialTheme.typography.bodySmall, color = TradingDown)
        }
    }
}

@Composable
private fun SignalCard(signal: SignalSnapshot) {
    AppCard {
        SectionHeader(title = "最新信号", subtitle = "后端只通过 WebSocket 下发一次授权信号")
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            MetricTile("交易对", signal.symbol, Modifier.weight(1f))
            MetricTile("信号状态", signalStateLabel(signal), Modifier.weight(1f), if (signal.tradeAuthorized) TradingUp else MutedText)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            MetricTile("建议金额", "${signal.suggestedStake ?: "-"} USDT", Modifier.weight(1f))
            MetricTile("置信度", confidenceLabel(signal.confidence), Modifier.weight(1f))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            StatusChip("授权：${authorizationLabel(signal.authorizationStatus, signal.tradeAuthorized)}", signal.tradeAuthorized, Modifier.weight(1f))
            StatusChip("自动下单：${if (signal.tradeAuthorized) "可用" else "不可用"}", signal.tradeAuthorized, Modifier.weight(1f))
        }
        Text("周期结束：${signal.cycleEnd ?: "-"}", style = MaterialTheme.typography.bodySmall, color = MutedText)
    }
}

@Composable
private fun AutomationCard(
    settings: AutomationSettings,
    debugState: AutomationDebugState,
    connectionState: String,
    lastSocketStatus: String,
    running: Boolean,
    status: String,
    tradingAccountReady: Boolean,
    onToggle: (Boolean) -> Unit,
    onModeChange: (String) -> Unit,
    onStart: () -> Unit,
    onCalibrate: () -> Unit,
    onStop: () -> Unit,
    onAccessibilityPermission: () -> Unit,
    onOverlayPermission: () -> Unit,
) {
    AppCard {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            SectionHeader(
                title = "自动化",
                subtitle = "识别事件合约页面后执行金额输入、按钮点击和确认",
                modifier = Modifier.weight(1f),
            )
            Switch(
                checked = settings.automationEnabled,
                enabled = tradingAccountReady || settings.automationEnabled,
                onCheckedChange = onToggle,
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            ModeButton("手动", settings.executionMode == "manual", Modifier.weight(1f)) { onModeChange("manual") }
            ModeButton(
                "辅助",
                settings.executionMode == "assisted",
                Modifier.weight(1f),
                enabled = tradingAccountReady,
            ) { onModeChange("assisted") }
            ModeButton(
                "自动",
                settings.executionMode == "automatic",
                Modifier.weight(1f),
                enabled = tradingAccountReady,
            ) { onModeChange("automatic") }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            StatusChip("交易账号 ${if (tradingAccountReady) "已验证" else "未验证"}", tradingAccountReady, Modifier.weight(1f))
            StatusChip("执行门槛 ${if (tradingAccountReady) "已放行" else "暂停"}", tradingAccountReady, Modifier.weight(1f))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            StatusChip("页面 ${debugState.screenName}", debugState.safeToAct, Modifier.weight(1f))
            StatusChip("识别 ${"%.0f".format(debugState.confidence * 100)}%", debugState.confidence >= 0.62, Modifier.weight(1f))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            StatusChip("WS ${wsConnectionLabel(connectionState)}", connectionState == "connected", Modifier.weight(1f))
            StatusChip("服务 ${if (running) "运行中" else "未启动"}", running, Modifier.weight(1f))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            ReadinessChip("金额框", debugState.amountFieldFound, Modifier.weight(1f))
            ReadinessChip("上涨", debugState.upButtonFound, Modifier.weight(1f))
            ReadinessChip("下跌", debugState.downButtonFound, Modifier.weight(1f))
        }
        Text("包名：${debugState.packageName}", style = MaterialTheme.typography.bodySmall, color = MutedText)
        Text("最后动作：${debugState.lastAction}", style = MaterialTheme.typography.bodySmall, color = BodyOnDark)
        Text("最近信号连接：${runtimeStatusLabel(lastSocketStatus)}", style = MaterialTheme.typography.bodySmall, color = MutedText)
        Text("状态：$status", style = MaterialTheme.typography.bodySmall, color = MutedText)
        Spacer(modifier = Modifier.height(2.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            PrimaryButton(
                text = if (running) "运行中" else "启动服务",
                modifier = Modifier.weight(1f),
                enabled = tradingAccountReady && settings.automationEnabled && !running,
                onClick = onStart,
            )
            SecondaryButton(text = "停止", modifier = Modifier.weight(1f), enabled = running, onClick = onStop)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            SecondaryButton(text = "识别页面", modifier = Modifier.fillMaxWidth(), enabled = running && tradingAccountReady, onClick = onCalibrate)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            SecondaryButton(text = "无障碍权限", modifier = Modifier.weight(1f), onClick = onAccessibilityPermission)
            SecondaryButton(text = "悬浮窗权限", modifier = Modifier.weight(1f), onClick = onOverlayPermission)
        }
    }
}

@Composable
private fun AppCard(content: @Composable ColumnScope.() -> Unit) {
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        shape = RoundedCornerShape(12.dp),
        border = BorderStroke(1.dp, HairlineDark),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
            content = content,
        )
    }
}

@Composable
private fun SectionHeader(title: String, subtitle: String, modifier: Modifier = Modifier) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        Text(subtitle, style = MaterialTheme.typography.bodySmall, color = MutedText)
    }
}

@Composable
private fun MetricTile(label: String, value: String, modifier: Modifier = Modifier, valueColor: Color = BodyOnDark) {
    Surface(color = SurfaceElevatedDark, shape = RoundedCornerShape(8.dp), modifier = modifier) {
        Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(label, style = MaterialTheme.typography.labelSmall, color = MutedText)
            Text(value, style = MaterialTheme.typography.titleMedium, color = valueColor, fontWeight = FontWeight.SemiBold)
        }
    }
}

@Composable
private fun StatusChip(label: String, active: Boolean, modifier: Modifier = Modifier) {
    Surface(
        color = if (active) BinanceYellow.copy(alpha = 0.16f) else SurfaceElevatedDark,
        contentColor = if (active) BinanceYellow else MutedText,
        shape = RoundedCornerShape(999.dp),
        border = BorderStroke(1.dp, if (active) BinanceYellow.copy(alpha = 0.55f) else HairlineDark),
        modifier = modifier,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.labelMedium,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
        )
    }
}

@Composable
private fun ReadinessChip(label: String, ready: Boolean, modifier: Modifier = Modifier) {
    val tone = if (ready) TradingUp else TradingDown
    Surface(
        color = tone.copy(alpha = 0.14f),
        contentColor = tone,
        shape = RoundedCornerShape(8.dp),
        border = BorderStroke(1.dp, tone.copy(alpha = 0.4f)),
        modifier = modifier,
    ) {
        Text(
            "$label：${if (ready) "是" else "否"}",
            style = MaterialTheme.typography.labelMedium,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
        )
    }
}

@Composable
private fun ModeButton(
    label: String,
    selected: Boolean,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    if (selected) {
        Button(
            onClick = onClick,
            modifier = modifier,
            enabled = enabled,
            colors = ButtonDefaults.buttonColors(containerColor = BinanceYellow, contentColor = Color(0xFF181A20)),
            shape = RoundedCornerShape(6.dp),
        ) {
            Text(label)
        }
    } else {
        SecondaryButton(text = label, modifier = modifier, enabled = enabled, onClick = onClick)
    }
}

@Composable
private fun PrimaryButton(text: String, modifier: Modifier = Modifier, enabled: Boolean = true, onClick: () -> Unit) {
    Button(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        colors = ButtonDefaults.buttonColors(containerColor = BinanceYellow, contentColor = Color(0xFF181A20)),
        shape = RoundedCornerShape(6.dp),
    ) {
        Text(text, fontWeight = FontWeight.SemiBold)
    }
}

@Composable
private fun SecondaryButton(text: String, modifier: Modifier = Modifier, enabled: Boolean = true, onClick: () -> Unit) {
    OutlinedButton(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        border = BorderStroke(1.dp, HairlineDark),
        shape = RoundedCornerShape(6.dp),
    ) {
        Text(text)
    }
}

private fun deviceFingerprint(context: Context): String {
    val androidId = Settings.Secure.getString(context.contentResolver, Settings.Secure.ANDROID_ID)
        ?.takeIf { it.isNotBlank() }
        ?: "unknown-device"
    return "android-$androidId".take(128).padEnd(16, '0')
}

private fun signalStateLabel(signal: SignalSnapshot): String =
    when {
        signal.id.isNullOrBlank() -> "等待"
        signal.tradeAuthorized -> "可执行"
        signal.authorizationStatus == "fee_reserve_insufficient" -> "预扣余额不足"
        signal.authorizationStatus in privateAccountGateStatuses -> "校验失败"
        else -> "等待授权"
    }

private fun modeLabel(mode: String): String =
    when (mode) {
        "manual" -> "手动"
        "assisted" -> "辅助"
        "automatic" -> "自动"
        else -> mode
    }

private fun confidenceLabel(raw: String): String =
    raw.toDoubleOrNull()?.let { "${(it * 100).roundToInt()}%" } ?: raw

private fun authorizationLabel(status: String?, authorized: Boolean): String =
    when {
        authorized -> "已通过"
        status.isNullOrBlank() -> "未授权"
        status == "page_not_ready" -> "页面未就绪"
        status == "fee_reserve_insufficient" -> "预扣余额不足"
        status in privateAccountGateStatuses -> "账号校验失败"
        status == "exchange_balance_insufficient" -> "自动下单暂不可用"
        else -> status
    }

private fun exchangeConnectionLabel(status: ExchangeCredentialStatus): String =
    when {
        status.connected && status.canReadAccount -> "已验证"
        status.verificationStatus == "missing" -> "未绑定"
        status.verificationStatus.isNotBlank() -> status.verificationStatus
        else -> "未绑定"
    }

private val privateAccountGateStatuses = setOf(
    "mismatch",
    "observed_balance_missing",
    "exchange_balance_missing",
    "exchange_balance_stale",
    "unknown",
)

private fun accountGateReady(status: String?): Boolean = status == "matched"

private fun accountGateLabel(status: String?): String =
    if (accountGateReady(status)) "已通过" else "校验失败"

private fun wsConnectionLabel(state: String): String = when {
    state == "connected" -> "正常"
    state == "connecting" -> "连接中"
    state == "request_failed" -> "请求失败"
    state == "failed" -> "异常"
    state.startsWith("closed:") -> "已关闭"
    state == "stopped" -> "已停止"
    else -> "未知"
}

private fun exchangeReady(status: ExchangeCredentialStatus): Boolean =
    status.connected && status.canReadAccount && status.verificationStatus == "verified"

private fun runtimeStatusLabel(status: String): String = when {
    status.startsWith("ws_signal_execute") -> "收到可执行信号"
    status.startsWith("ws_signal_duplicate_ignored") -> "重复信号已忽略"
    status.startsWith("ws_signal_ignored") -> "信号不可执行"
    status == "ws_signal_not_authorized_fee_reserve_insufficient" -> "预扣余额不足，已暂停"
    status == "ws_signal_not_authorized_account_gate" -> "账号校验失败，已暂停"
    status == "ws_signal_not_authorized" -> "信号未授权"
    status == "trading_account_not_verified" -> "交易账号未验证"
    status.startsWith("confirmed") -> "已确认下单"
    status.startsWith("order_reported") -> "订单已上报"
    status.startsWith("action_button_not_found") -> "执行失败：按钮未找到"
    status.startsWith("action_click_failed") -> "执行失败：按钮点击失败"
    status.startsWith("amount_prepared_waiting_confirmation") -> "金额已填写，等待确认"
    else -> status
}
