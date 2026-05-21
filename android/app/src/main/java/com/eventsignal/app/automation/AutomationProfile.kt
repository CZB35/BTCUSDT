package com.eventsignal.app.automation

data class AutomationProfile(
    val id: String,
    val targetPackageNames: List<String>,
    val minRecognitionConfidence: Double,
    val pageKeywords: List<String>,
    val amountFieldHints: List<String>,
    val upButtonKeywords: List<String>,
    val downButtonKeywords: List<String>,
    val confirmButtonKeywords: List<String>,
)

object BuiltInProfiles {
    val BinanceEventContractV1 = AutomationProfile(
        id = "binance_event_contract_v1",
        targetPackageNames = listOf(
            "com.binance.dev",
            "com.binance.client",
            "com.binance.mobile",
        ),
        minRecognitionConfidence = 0.62,
        pageKeywords = listOf(
            "BTCUSDT",
            "Event Contract",
            "事件",
            "合约",
            "事件合约",
            "时间单位",
            "1分",
            "5分",
            "10m",
            "10分钟",
            "30m",
            "30分钟",
            "15分",
            "30分",
            "1小时",
            "数量",
            "数量(USDT)",
            "支付率",
            "支付金额",
            "上涨",
            "下跌",
            "看涨",
            "看跌",
        ),
        amountFieldHints = listOf(
            "Amount",
            "Stake",
            "USDT",
            "数量",
            "数量(USDT)",
            "金额",
            "下注",
            "投入",
            "支付金额",
        ),
        upButtonKeywords = listOf(
            "UP",
            "Up",
            "CALL",
            "Buy Up",
            "看涨",
            "上涨",
            "上 涨",
            "涨",
        ),
        downButtonKeywords = listOf(
            "DOWN",
            "Down",
            "PUT",
            "Buy Down",
            "看跌",
            "下跌",
            "下 跌",
            "跌",
        ),
        confirmButtonKeywords = listOf(
            "确认",
            "確定",
            "确定",
            "确认下单",
            "确定下单",
            "Confirm",
            "CONFIRM",
        ),
    )

    fun byId(id: String?): AutomationProfile =
        when (id) {
            BinanceEventContractV1.id, null -> BinanceEventContractV1
            else -> BinanceEventContractV1
        }
}
