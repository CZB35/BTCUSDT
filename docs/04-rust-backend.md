# Rust Backend

## Recommended Crate Stack

```toml
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono", "rust_decimal"] }
redis = { version = "0.25", features = ["tokio-comp"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1", features = ["v7", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
rust_decimal = { version = "1", features = ["serde"] }
jsonwebtoken = "9"
argon2 = "0.5"
```

Use `alloy-rs` for BSC RPC integration if available in the implementation environment.

## Service Layout

```text
backend/
  crates/
    api-server/
    domain/
    market-data/
    prediction/
    chain-watcher/
    notification/
    settlement/
    worker-common/
  migrations/
  config/
```

## HTTP API

Use Axum for the API server:

- Authentication middleware.
- Request ID middleware.
- Rate limit middleware.
- JSON extractor validation.
- Structured errors.

## Workers

### MarketDataWorker

- Pull BTCUSDT 1-minute K-lines.
- Validate continuity.
- Store closed candles.
- Update Redis latest market cache.

### PredictionWorker

- Load latest candles.
- Compute features.
- Generate candidate prediction.
- Apply risk filters.
- Store signal.

### SignalScheduler

- Align signals to 10-minute cycles.
- Avoid duplicate cycle signals.
- Publish to queue.

### NotificationWorker

- Consume signal events.
- Push through WebSocket gateway.
- Send FCM fallback.

### BscDepositWatcher

- Scan USDT Transfer logs.
- Match platform receiving address.
- Validate sender against bound wallet.
- Confirm block depth.
- Credit ledger.

### SettlementWorker

- Consume execution reports.
- Validate report consistency.
- Validate Android automation source, action trace, and reported status where available.
- Calculate profit share.
- Write settlement and ledger entries atomically.

### UserSettingsService

- Store user trading preferences.
- Validate stake percentage range.
- Return normalized settings to Android.
- Provide settings to signal delivery so suggested stake is calculated consistently.
- Store automation preferences and expose supported execution modes to Android.

First-release defaults:

```text
stake_percent = 0.05
min_stake_usdt = 10
min_confidence = platform default
signal_alert_enabled = true
floating_window_enabled = true
automation_enabled = false
execution_mode = manual
require_final_confirmation = true
```

## Configuration

Use environment variables:

```text
APP_ENV=production
DATABASE_URL=postgres://...
REDIS_URL=redis://...
JWT_SECRET=...
BSC_RPC_URL=https://...
BSC_USDT_CONTRACT=0x55d398326f99059fF775485246999027B3197955
PLATFORM_BSC_RECEIVER=0x...
DEPOSIT_MIN_CONFIRMATIONS=15
PROFIT_SHARE_RATE=0.30
MARKET_DATA_PRIMARY=https://data-api.binance.vision
FCM_SERVICE_ACCOUNT_PATH=/secrets/fcm.json
```

## Error Handling

Use domain-specific error codes:

```text
AUTH_INVALID_TOKEN
WALLET_NOT_BOUND
DEPOSIT_TX_NOT_FOUND
DEPOSIT_FROM_ADDRESS_MISMATCH
DEPOSIT_TO_ADDRESS_MISMATCH
DEPOSIT_ALREADY_USED
INSUFFICIENT_BALANCE
SIGNAL_STALE
EXECUTION_REPORT_DUPLICATE
SETTLEMENT_REVIEW_REQUIRED
USER_SETTING_OUT_OF_RANGE
AUTOMATION_SETTING_OUT_OF_RANGE
EXECUTION_TRACE_REQUIRED
EXECUTION_SOURCE_INVALID
```
