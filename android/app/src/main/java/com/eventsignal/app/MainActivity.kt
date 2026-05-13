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
    var automationRunning by remember { mutableStateOf(false) }
    var status by remember { mutableStateOf("就绪") }
    var baseUrl by remember { mutableStateOf(repository.baseUrl()) }
    var accessToken by remember { mutableStateOf(repository.accessToken().orEmpty()) }
    var walletAddress by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    val walletDeepLink = (context as? Activity)?.intent?.data?.getQueryParameter("address").orEmpty()

    suspend fun refresh() {
        val (nextSignal, nextSettings) = withContext(Dispatchers.IO) {
            repository.loadLatestSignal().getOrDefault(SignalSnapshot.empty()) to
                repository.loadAutomationSettings().getOrDefault(stateStore.loadSettings())
        }
        signal = nextSignal
        automationSettings = nextSettings
        debugState = stateStore.loadDebugState()
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
                    baseUrl = baseUrl,
                    accessToken = accessToken,
                    walletAddress = walletAddress,
                    password = password,
                    status = status,
                    onBaseUrlChange = { baseUrl = it },
                    onWalletAddressChange = { walletAddress = it },
                    onPasswordChange = { password = it },
                    onSaveConnection = {
                        repository.saveConnection(baseUrl, accessToken)
                        status = "接口地址已保存"
                    },
                    onOpenDapp = {
                        repository.saveConnection(baseUrl, accessToken)
                        val url = "${baseUrl.trimEnd('/')}/dapp/?return_uri=eventsignal://wallet"
                        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
                        status = "已打开充值 DApp"
                    },
                    onLogin = {
                        repository.saveConnection(baseUrl, accessToken)
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
                        repository.saveConnection(baseUrl, accessToken)
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
                )
                SignalCard(signal)
                AutomationCard(
                    settings = automationSettings,
                    debugState = debugState,
                    running = automationRunning,
                    status = status,
                    onToggle = { enabled ->
                        persistAutomationSettings(automationSettings.copy(automationEnabled = enabled))
                        status = if (enabled) "自动化已开启" else "自动化已关闭"
                    },
                    onModeChange = { mode ->
                        persistAutomationSettings(automationSettings.copy(executionMode = mode))
                        status = "执行模式已切换为${modeLabel(mode)}"
                    },
                    onStart = {
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

@Composable
private fun AuthCard(
    baseUrl: String,
    accessToken: String,
    walletAddress: String,
    password: String,
    status: String,
    onBaseUrlChange: (String) -> Unit,
    onWalletAddressChange: (String) -> Unit,
    onPasswordChange: (String) -> Unit,
    onSaveConnection: () -> Unit,
    onOpenDapp: () -> Unit,
    onLogin: () -> Unit,
    onRegister: () -> Unit,
) {
    AppCard {
        SectionHeader(title = "账号连接", subtitle = "登录后接收后端 WebSocket 信号")
        OutlinedTextField(
            value = baseUrl,
            onValueChange = onBaseUrlChange,
            label = { Text("后端地址") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = walletAddress,
            onValueChange = onWalletAddressChange,
            label = { Text("钱包地址") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = password,
            onValueChange = onPasswordChange,
            label = { Text("密码") },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            modifier = Modifier.fillMaxWidth(),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            PrimaryButton(text = "登录", modifier = Modifier.weight(1f), onClick = onLogin)
            SecondaryButton(text = "注册", modifier = Modifier.weight(1f), onClick = onRegister)
        }
        SecondaryButton(text = "连接钱包 / 充值", modifier = Modifier.fillMaxWidth(), onClick = onOpenDapp)
        SecondaryButton(text = "仅保存接口地址", modifier = Modifier.fillMaxWidth(), onClick = onSaveConnection)
        StatusChip(
            label = if (accessToken.isBlank()) "未登录" else "令牌已保存",
            active = accessToken.isNotBlank(),
            modifier = Modifier.fillMaxWidth(),
        )
        Text(status, style = MaterialTheme.typography.bodySmall, color = MutedText)
    }
}

@Composable
private fun SignalCard(signal: SignalSnapshot) {
    val directionTone = when (signal.direction.uppercase()) {
        "UP" -> TradingUp
        "DOWN" -> TradingDown
        else -> MutedText
    }
    AppCard {
        SectionHeader(title = "最新信号", subtitle = "后端只通过 WebSocket 下发一次授权信号")
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            MetricTile("交易对", signal.symbol, Modifier.weight(1f))
            MetricTile("方向", directionLabel(signal.direction), Modifier.weight(1f), directionTone)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            MetricTile("建议金额", "${signal.suggestedStake ?: "-"} USDT", Modifier.weight(1f))
            MetricTile("置信度", confidenceLabel(signal.confidence), Modifier.weight(1f))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            StatusChip("授权：${authorizationLabel(signal.authorizationStatus, signal.tradeAuthorized)}", signal.tradeAuthorized, Modifier.weight(1f))
            StatusChip("预扣：${signal.feeReserveUsdt ?: "-"} USDT", signal.feeReserveUsdt != null, Modifier.weight(1f))
        }
        Text("周期结束：${signal.cycleEnd ?: "-"}", style = MaterialTheme.typography.bodySmall, color = MutedText)
    }
}

@Composable
private fun AutomationCard(
    settings: AutomationSettings,
    debugState: AutomationDebugState,
    running: Boolean,
    status: String,
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
                subtitle = "识别事件合约页面后执行金额输入、方向点击和确认",
                modifier = Modifier.weight(1f),
            )
            Switch(checked = settings.automationEnabled, onCheckedChange = onToggle)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            ModeButton("手动", settings.executionMode == "manual", Modifier.weight(1f)) { onModeChange("manual") }
            ModeButton("辅助", settings.executionMode == "assisted", Modifier.weight(1f)) { onModeChange("assisted") }
            ModeButton("自动", settings.executionMode == "automatic", Modifier.weight(1f)) { onModeChange("automatic") }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            StatusChip("页面 ${debugState.screenName}", debugState.safeToAct, Modifier.weight(1f))
            StatusChip("识别 ${"%.0f".format(debugState.confidence * 100)}%", debugState.confidence >= 0.62, Modifier.weight(1f))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            ReadinessChip("金额框", debugState.amountFieldFound, Modifier.weight(1f))
            ReadinessChip("上涨", debugState.upButtonFound, Modifier.weight(1f))
            ReadinessChip("下跌", debugState.downButtonFound, Modifier.weight(1f))
        }
        Text("包名：${debugState.packageName}", style = MaterialTheme.typography.bodySmall, color = MutedText)
        Text("最后动作：${debugState.lastAction}", style = MaterialTheme.typography.bodySmall, color = BodyOnDark)
        Text("状态：$status", style = MaterialTheme.typography.bodySmall, color = MutedText)
        Spacer(modifier = Modifier.height(2.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            PrimaryButton(
                text = if (running) "运行中" else "启动服务",
                modifier = Modifier.weight(1f),
                enabled = settings.automationEnabled && !running,
                onClick = onStart,
            )
            SecondaryButton(text = "停止", modifier = Modifier.weight(1f), enabled = running, onClick = onStop)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            SecondaryButton(text = "识别页面", modifier = Modifier.fillMaxWidth(), enabled = running, onClick = onCalibrate)
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
private fun ModeButton(label: String, selected: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    if (selected) {
        Button(
            onClick = onClick,
            modifier = modifier,
            colors = ButtonDefaults.buttonColors(containerColor = BinanceYellow, contentColor = Color(0xFF181A20)),
            shape = RoundedCornerShape(6.dp),
        ) {
            Text(label)
        }
    } else {
        SecondaryButton(text = label, modifier = modifier, onClick = onClick)
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

private fun directionLabel(direction: String): String =
    when (direction.uppercase()) {
        "UP" -> "看涨"
        "DOWN" -> "看跌"
        "HOLD" -> "观望"
        else -> direction.ifBlank { "-" }
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
        status == "insufficient_balance" -> "余额不足"
        else -> status
    }
