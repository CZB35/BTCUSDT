# System Architecture

## High-Level Components

```text
Android App
  - Compose UI
  - Foreground service
  - Floating window service
  - Automation service
  - Screen recognition and action runner
  - WebSocket signal client
  - Push notification client
  - Wallet binding flow
  - Execution report submission client

Rust Backend
  - HTTP API
  - WebSocket gateway
  - Market data worker
  - Prediction worker
  - Signal scheduler
  - Notification worker
  - BSC deposit watcher
  - Ledger service
  - Settlement service
  - Admin API

Infrastructure
  - PostgreSQL
  - Redis
  - NATS JetStream or Redis Streams
  - Object storage for audit images/logs if needed
  - FCM for Android push
  - BSC RPC provider
```

## Recommended Stack

- Android: Kotlin, Jetpack Compose, Coroutines, Room optional, OkHttp, Kotlinx Serialization.
- Backend: Rust, Axum, Tokio, sqlx, tower, tracing, serde, reqwest.
- Database: PostgreSQL 16+.
- Cache and locks: Redis 7+.
- Queue: NATS JetStream preferred; Redis Streams acceptable for first release.
- Chain: alloy-rs or ethers-rs. Prefer alloy-rs for new Rust development.
- Observability: Prometheus, Grafana, Loki or OpenTelemetry collector.

## Runtime Data Flow

```text
Binance market data source
  -> MarketDataWorker
  -> candles table / Redis latest cache
  -> PredictionWorker
  -> signals table
  -> SignalScheduler
  -> WebSocketGateway + NotificationWorker
  -> Android floating window

BSC chain
  -> BscDepositWatcher
  -> deposits table
  -> LedgerService
  -> balances table
  -> Android wallet page

Android automation service
  -> third-party event contract app UI
  -> execution_reports API
  -> SettlementWorker
  -> settlements + ledger_entries + profit_share_fees

Manual reconciliation flow
  -> execution_reports API
  -> SettlementWorker
  -> settlements + ledger_entries + profit_share_fees
```

## Boundary For Execution

Execution is split between Android and backend responsibilities. The Android app owns UI recognition, action sequencing, local safety controls, and execution report capture. The backend owns signal generation, user settings, balance gating, report storage, settlement, and audit review.

This boundary keeps core systems clean:

- Prediction is independent.
- Accounting is independent.
- Signal delivery is independent.
- UI automation details stay in Android automation modules, not backend workers.
- Settlement uses execution reports and audit fields rather than direct UI state.

## Failure Philosophy

- If market data is stale, do not emit tradeable signals.
- If prediction confidence is below threshold, emit `HOLD` instead of `UP` or `DOWN`.
- If chain confirmation is insufficient, do not credit balance.
- If execution report is inconsistent, mark it as `REVIEW_REQUIRED`.
- If ledger operation cannot complete atomically, rollback the transaction.
- If Android automation loses screen confidence or target app state is unknown, stop the action sequence and report `FAILED_OR_UNCERTAIN`.
