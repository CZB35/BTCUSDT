package com.eventsignal.app.data

import com.eventsignal.app.domain.SignalSnapshot
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject

object SignalJsonParser {
    private val json = Json {
        ignoreUnknownKeys = true
    }

    fun parse(raw: String): SignalSnapshot {
        val root = json.parseToJsonElement(raw)
        val data = root.jsonObject["data"]?.takeIf { it is JsonObject } ?: root
        val obj = data.jsonObject
        return SignalSnapshot(
            id = obj["id"].asTextOrNull(),
            symbol = obj["symbol"].asTextOrNull() ?: "BTCUSDT",
            direction = obj["direction"].asTextOrNull() ?: "HOLD",
            confidence = obj["confidence"].asTextOrNull() ?: "-",
            suggestedStake = obj["suggested_stake"].asTextOrNull(),
            tradeAuthorized = obj["trade_authorized"].asBooleanOrFalse(),
            reservationId = obj["reservation_id"].asTextOrNull(),
            feeReserveUsdt = obj["fee_reserve_usdt"].asTextOrNull(),
            authorizationStatus = obj["authorization_status"].asTextOrNull(),
            cycleEnd = obj["cycle_end"].asTextOrNull(),
        )
    }

    private fun JsonElement?.asTextOrNull(): String? {
        if (this == null || this is JsonNull) return null
        return if (this is JsonPrimitive && isString) {
            content
        } else {
            toString()
        }.takeIf { it.isNotBlank() }
    }

    private fun JsonElement?.asBooleanOrFalse(): Boolean =
        (this as? JsonPrimitive)?.let { primitive ->
            primitive.content.equals("true", ignoreCase = true)
        } ?: false
}
