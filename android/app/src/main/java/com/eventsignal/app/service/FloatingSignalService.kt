package com.eventsignal.app.service

import android.app.Service
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.PixelFormat
import android.os.Build
import android.os.IBinder
import android.provider.Settings
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import com.eventsignal.app.automation.AutomationStateStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

class FloatingSignalService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private lateinit var windowManager: WindowManager
    private lateinit var stateStore: AutomationStateStore
    private var overlayView: View? = null
    private var params: WindowManager.LayoutParams? = null
    private var collapsed = false

    override fun onCreate() {
        super.onCreate()
        windowManager = getSystemService(WindowManager::class.java)
        stateStore = AutomationStateStore(applicationContext)
        startForeground(1003, runtimeNotification(this, "\u60ac\u6d6e\u7a97", "\u4e8b\u4ef6\u4fe1\u53f7\u60ac\u6d6e\u7a97\u8fd0\u884c\u4e2d"))
        showOverlayIfAllowed()
        startRefreshLoop()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        showOverlayIfAllowed()
        return START_STICKY
    }

    override fun onDestroy() {
        scope.cancel()
        overlayView?.let { view ->
            runCatching { windowManager.removeView(view) }
        }
        overlayView = null
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun showOverlayIfAllowed() {
        if (!Settings.canDrawOverlays(this) || overlayView != null) return
        val view = buildOverlayView()
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY
        } else {
            @Suppress("DEPRECATION")
            WindowManager.LayoutParams.TYPE_PHONE
        }
        val layoutParams = WindowManager.LayoutParams(
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.WRAP_CONTENT,
            type,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.TOP or Gravity.START
            x = 24
            y = 180
        }
        params = layoutParams
        overlayView = view
        runCatching {
            windowManager.addView(view, layoutParams)
        }.onFailure {
            overlayView = null
            params = null
            stateStore.saveLastStatus("overlay_add_failed:${it.javaClass.simpleName}")
            stopSelf()
        }
    }

    private fun buildOverlayView(): View {
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(16, 12, 16, 12)
            setBackgroundColor(0xF01E2329.toInt())
        }
        val title = TextView(this).apply {
            tag = "title"
            text = "BTCUSDT 10m"
            setTextColor(0xFFEAECEF.toInt())
            textSize = 14f
            typeface = Typeface.DEFAULT_BOLD
        }
        val signal = TextView(this).apply {
            tag = "signal"
            setTextColor(0xFFFCD535.toInt())
            textSize = 12f
        }
        val status = TextView(this).apply {
            tag = "status"
            setTextColor(0xFF929AA5.toInt())
            textSize = 11f
        }
        val actions = LinearLayout(this).apply {
            tag = "actions"
            orientation = LinearLayout.HORIZONTAL
        }
        val toggle = Button(this).apply {
            tag = "toggle"
            text = "\u6536"
            styleOverlayButton()
            setOnClickListener {
                collapsed = !collapsed
                applyCollapsedState(container)
            }
        }
        val stop = Button(this).apply {
            text = "\u505c"
            styleOverlayButton()
            setOnClickListener {
                AutomationRuntime.stop()
                stopService(Intent(this@FloatingSignalService, SignalForegroundService::class.java))
                stopService(Intent(this@FloatingSignalService, AutomationForegroundService::class.java))
                stopSelf()
            }
        }
        container.addView(title)
        container.addView(signal)
        container.addView(status)
        actions.addView(toggle)
        actions.addView(stop)
        container.addView(actions)
        attachDrag(container)
        return container
    }

    private fun applyCollapsedState(container: LinearLayout) {
        val signal = container.findViewWithTag<TextView>("signal")
        val status = container.findViewWithTag<TextView>("status")
        val actions = container.findViewWithTag<LinearLayout>("actions")
        val toggle = container.findViewWithTag<Button>("toggle")
        signal.visibility = if (collapsed) View.GONE else View.VISIBLE
        status.visibility = if (collapsed) View.GONE else View.VISIBLE
        for (index in 0 until actions.childCount) {
            val child = actions.getChildAt(index)
            child.visibility = if (!collapsed || child === toggle) View.VISIBLE else View.GONE
        }
        toggle.text = if (collapsed) "\u5c55" else "\u6536"
        overlayView?.let { view ->
            params?.let { layoutParams ->
                runCatching { windowManager.updateViewLayout(view, layoutParams) }
            }
        }
    }

    private fun attachDrag(view: View) {
        var startX = 0
        var startY = 0
        var touchX = 0f
        var touchY = 0f
        view.setOnTouchListener { _, event ->
            val layoutParams = params ?: return@setOnTouchListener false
            when (event.action) {
                MotionEvent.ACTION_DOWN -> {
                    startX = layoutParams.x
                    startY = layoutParams.y
                    touchX = event.rawX
                    touchY = event.rawY
                    true
                }
                MotionEvent.ACTION_MOVE -> {
                    layoutParams.x = startX + (event.rawX - touchX).toInt()
                    layoutParams.y = startY + (event.rawY - touchY).toInt()
                    overlayView?.let { overlay ->
                        runCatching { windowManager.updateViewLayout(overlay, layoutParams) }
                    }
                    true
                }
                else -> false
            }
        }
    }

    private fun startRefreshLoop() {
        scope.launch {
            while (isActive) {
                refreshOverlayText()
                delay(1_000)
            }
        }
    }

    private fun refreshOverlayText() {
        val view = overlayView ?: return
        val signal = stateStore.loadSignal()
        val debug = stateStore.loadDebugState()
        val connection = stateStore.loadConnectionState()
        val summary = stateStore.loadRuntimeSummary()
        val signalText = view.findViewWithTag<TextView>("signal")
        val statusText = view.findViewWithTag<TextView>("status")
        signalText.text =
            "下单:${summary.ordersTotal}次  胜率:${percent(summary.winRate)}(${summary.wins}/${summary.wins + summary.losses})\n" +
            "盈亏:${summary.pnlUsdt}U  余额:${debug.observedBalanceUsdt ?: "-"}U"
        statusText.text =
            "下次:${signal?.suggestedStake ?: "-"}U  待结算:${summary.pendingSettlements}笔\n" +
            "识别:${recognitionLabel(debug)}  WS:${wsLabel(connection)}\n" +
            "状态:${runtimeStatusLabel(stateStore.loadLastStatus())}  " +
            if (AutomationRuntime.running) "\u8fd0\u884c\u4e2d" else "\u5df2\u505c"
    }

    private fun recognitionLabel(debug: com.eventsignal.app.automation.AutomationDebugState): String =
        if (debug.safeToAct) "\u5df2\u8bc6\u522b" else "\u672a\u8bc6\u522b"

    private fun wsLabel(state: String): String = when {
        state == "connected" -> "\u6b63\u5e38"
        state == "connecting" -> "\u8fde\u63a5\u4e2d"
        state == "request_failed" -> "\u8bf7\u6c42\u5931\u8d25"
        state == "failed" -> "\u5f02\u5e38"
        state.startsWith("closed:") -> "\u5df2\u5173\u95ed"
        state == "stopped" -> "\u5df2\u505c"
        else -> "\u672a\u77e5"
    }

    private fun yesNo(value: Boolean): String = if (value) "\u662f" else "\u5426"

    private fun percent(raw: String): String =
        raw.toDoubleOrNull()?.let { "${(it * 100).toInt()}%" } ?: "0%"

    private fun runtimeStatusLabel(status: String): String = when {
        status.startsWith("ws_signal_execute") -> "收到信号"
        status == "ws_signal_not_authorized_fee_reserve_insufficient" -> "预扣不足"
        status == "ws_signal_not_authorized_account_gate" -> "校验失败"
        status == "ws_signal_not_authorized" -> "未授权"
        status == "trading_account_not_verified" -> "未验证"
        status.startsWith("confirmed") -> "已确认"
        status.startsWith("order_reported") -> "已上报"
        status.startsWith("action_button_not_found") -> "按钮缺失"
        status.startsWith("action_click_failed") -> "点击失败"
        else -> status
    }

    private fun Button.styleOverlayButton() {
        textSize = 11f
        minHeight = 0
        minWidth = 0
        minimumHeight = 0
        minimumWidth = 0
        includeFontPadding = false
        setPadding(14, 6, 14, 6)
        setTextColor(Color.rgb(24, 26, 32))
        setBackgroundColor(0xFFFCD535.toInt())
    }

}
