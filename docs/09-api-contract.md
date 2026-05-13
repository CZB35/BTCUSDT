# API Contract

All APIs use JSON over HTTPS. All authenticated endpoints require `Authorization: Bearer <token>`.

## Auth

### POST /v1/auth/register

```json
{
  "email": "user@example.com",
  "password": "secret",
  "device_fingerprint": "android-device-id"
}
```

Register is rate limited per email and per IP to reduce scripted bulk signups. A blocked request returns `429` with `AUTH_RATE_LIMITED`.

### POST /v1/auth/login

```json
{
  "email": "user@example.com",
  "password": "secret",
  "device_fingerprint": "android-device-id"
}
```

Response:

```json
{
  "access_token": "...",
  "refresh_token": "...",
  "expires_in": 3600
}
```

Only one active device session is allowed per account. If another device is already active, login returns `ACCOUNT_ALREADY_LOGGED_IN_ON_ANOTHER_DEVICE`. The active device must logout before another device can login.

## Wallet Binding

### POST /v1/wallets/bind-message

```json
{
  "chain": "bsc",
  "address": "0x..."
}
```

Response:

```json
{
  "message": "Bind wallet to BTCUSDT Event Signal Platform\nUser ID: ...\nNonce: ...",
  "nonce": "..."
}
```

### POST /v1/wallets/verify

```json
{
  "chain": "bsc",
  "address": "0x...",
  "message": "...",
  "signature": "0x..."
}
```

## Deposits

Deposits require an authenticated account. The credited balance belongs to the account that submitted/confirmed the deposit, not to a standalone wallet session.

### GET /v1/deposits/instructions

Response:

```json
{
  "chain": "bsc",
  "asset": "USDT",
  "token_contract": "0x55d398326f99059fF775485246999027B3197955",
  "platform_receiver": "0x...",
  "bound_wallet": "0x...",
  "min_amount": "10.0"
}
```

### POST /v1/deposits/submit-tx

```json
{
  "tx_hash": "0x..."
}
```

### GET /v1/deposits

Returns user deposit history.

## Balance

### GET /v1/balance

```json
{
  "asset": "USDT",
  "available": "100.00000000",
  "frozen": "0.00000000",
  "activation_status": "active"
}
```

## User Trading Settings

### GET /v1/user-settings/trading

Response:

```json
{
  "stake_percent": "0.050000",
  "min_stake_usdt": "10.00",
  "max_single_stake_usdt": null,
  "min_confidence": null,
  "signal_alert_enabled": true,
  "floating_window_enabled": true
}
```

### PUT /v1/user-settings/trading

Request:

```json
{
  "stake_percent": "0.100000",
  "max_single_stake_usdt": "50.00",
  "min_confidence": "0.580000",
  "signal_alert_enabled": true,
  "floating_window_enabled": true
}
```

Validation:

```text
stake_percent must be between 0.01 and 0.20
max_single_stake_usdt must be null or between 5 and 250
min_confidence must be null or between 0 and 1
```

Response:

```json
{
  "stake_percent": "0.100000",
  "min_stake_usdt": "5.00",
  "max_single_stake_usdt": "50.00",
  "min_confidence": "0.580000",
  "signal_alert_enabled": true,
  "floating_window_enabled": true
}
```

## Automation Settings

### GET /v1/user-settings/automation

Response:

```json
{
  "automation_enabled": false,
  "execution_mode": "manual",
  "target_app_profile": "binance_event_contract_v1",
  "require_final_confirmation": true,
  "emergency_stop_enabled": true
}
```

### PUT /v1/user-settings/automation

Request:

```json
{
  "automation_enabled": true,
  "execution_mode": "assisted",
  "target_app_profile": "binance_event_contract_v1",
  "require_final_confirmation": true
}
```

Validation:

```text
execution_mode must be manual, assisted, or automatic
automatic mode requires automation_enabled = true
target_app_profile must match a supported Android automation profile
```

## Automation Page Gate

### POST /v1/automation/page-state

Android reports this before any signal arrives. The backend treats the page gate as open only while the latest report is fresh, default 3 seconds.

```json
{
  "page_ready": true,
  "screen_name": "event_contract",
  "confidence": "1.000000",
  "package_name": "com.binance.dev",
  "amount_field_found": true,
  "up_button_found": true,
  "down_button_found": true,
  "observed_at": "2026-05-11T10:00:00Z"
}
```

## Signals

### GET /v1/signals/latest

This endpoint is display-only. It never freezes balance and must not trigger Android automation. Production automatic execution is only driven by the WebSocket `signal.created` event.

```json
{
  "id": "018f...",
  "symbol": "BTCUSDT",
  "period_seconds": 600,
  "cycle_start": "2026-05-11T10:00:00Z",
  "cycle_end": "2026-05-11T10:10:00Z",
  "direction": "UP",
  "confidence": "0.610000",
  "predicted_return": "0.0008000000",
  "entry_price": "81100.00",
  "suggested_stake": "20.00",
  "stake_percent": "0.050000",
  "suggested_stake_rule": "clamp(max(balance * user_stake_percent, 5 USDT), 5, 250 USDT); requires fee reserve = expected_profit * 30%",
  "trade_authorized": false,
  "reservation_id": null,
  "fee_reserve_usdt": "4.80000000",
  "authorization_status": "not_reserved",
  "balance_available_after_reserve": "100.00000000",
  "model_version": "score-v1"
}
```

### GET /v1/signals/history

Paginated signal history.

## WebSocket

### GET /v1/ws

Client subscribes after auth. Server sends:

The backend authorizes and freezes fee reserve only while building this event. Required gates are: WebSocket connected, automatic mode enabled, final confirmation disabled, fresh page gate ready, and sufficient fee-reserve balance. If the app is disconnected or receives `connection.lagged`, it must not request old signals for compensation; missed signals are skipped.

```json
{
  "type": "signal.created",
  "data": {
    "signal_id": "018f...",
    "symbol": "BTCUSDT",
    "direction": "UP",
    "confidence": "0.61",
    "suggested_stake": "20.00",
    "stake_percent": "0.050000",
    "trade_authorized": true,
    "reservation_id": "018f...",
    "fee_reserve_usdt": "4.80000000",
    "cycle_end": "2026-05-11T10:10:00Z"
  }
}
```

If `trade_authorized=false`, Android must not auto-click. Common `authorization_status` values are `stake_balance_insufficient`, `fee_reserve_insufficient`, `page_state_missing`, `page_state_stale`, `page_not_ready`, `automation_gate_closed`, `not_reserved`, and `reserved`.

## Event Contract Orders

### POST /v1/event-contract-orders

Android sends this immediately after the confirmation dialog is clicked. Automatic production orders must include `reservation_id`.

```json
{
  "client_order_id": "android-...",
  "source": "android_automation",
  "reservation_id": "018f...",
  "signal_id": "018f...",
  "symbol": "BTCUSDT",
  "period_seconds": 600,
  "direction": "DOWN",
  "stake_usdt": "5.00",
  "automation_profile": "binance_event_contract_v1",
  "automation_confidence": "1.000000",
  "action_trace_id": "android-...",
  "reported_at": "2026-05-11T10:00:05Z"
}
```

## Execution Reports

This endpoint records manual or Android automation execution results for settlement, audit review, and profit-share accounting.

### POST /v1/execution-reports

```json
{
  "client_report_id": "device-generated-uuid",
  "source": "android_automation",
  "signal_id": "018f...",
  "symbol": "BTCUSDT",
  "period_seconds": 600,
  "direction": "UP",
  "stake_usdt": "20.00",
  "entry_price": "81100.00",
  "expiry_price": "81200.00",
  "payout_ratio": "0.80",
  "gross_profit": "16.00",
  "result": "WIN",
  "automation_profile": "binance_event_contract_v1",
  "automation_confidence": "0.940000",
  "action_trace_id": "local-trace-id",
  "reported_at": "2026-05-11T10:10:05Z"
}
```

For losing reports, send negative `gross_profit` when available. If unavailable, backend treats the loss amount as `stake_usdt` for the 30% loss rebate.

Response:

```json
{
  "report_id": "018f...",
  "status": "pending_review"
}
```

## Settlements

### GET /v1/settlements

Returns user settlement history.

```json
[
  {
    "id": "018f...",
    "execution_report_id": "018f...",
    "gross_profit": "-5.00",
    "profit_share_fee": "0.00",
    "loss_rebate": "1.50",
    "user_net_profit": "-3.50",
    "status": "settled",
    "created_at": "2026-05-11T10:10:05Z"
  }
]
```
