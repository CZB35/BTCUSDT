# Roadmap

## Phase 0: Foundations

- Finalize business rules.
- Finalize platform receiving wallet and custody policy.
- Create repository structure.
- Create database migrations.
- Set up local Docker Compose.

## Phase 1: Backend MVP

- Auth and user system.
- Device registration.
- Wallet binding by signature.
- Balance and ledger service.
- Deposit tx submission API.
- BSC USDT watcher.
- Admin read APIs for users, deposits, ledger.

## Phase 2: Market And Prediction

- BTCUSDT 1m K-line collector.
- Candle continuity and stale-data checks.
- Baseline prediction model.
- Signal scheduler.
- Prediction result evaluator.
- Backtest command-line tool.

## Phase 3: Signal Delivery

- WebSocket gateway.
- FCM integration.
- User permission gating by balance/activation.
- Signal history APIs.

## Phase 4: Android MVP

- Compose app shell.
- Login.
- Wallet binding.
- Recharge instructions.
- Balance and ledger history.
- Signal dashboard.
- Floating signal window.
- Foreground service and notification.
- Automation settings page.
- Automation foreground service foundation.
- Target app profile storage.
- Execution report upload from Android.

## Phase 5: Android Automation MVP

- Screen recognition prototype.
- Action sequence runner.
- Automatic execution mode only.
- Emergency stop from floating window and notification.
- Local action trace and audit export.
- Failure handling for unknown screens, expired signals, and low recognition confidence.

## Phase 6: Settlement And Profit Share

- Execution report API.
- Settlement worker.
- Profit-share fee calculation.
- Ledger fee deduction.
- Review-required flow.
- Admin settlement inspection.

## Phase 7: Hardening

- Security review.
- Load testing.
- Chain watcher reorg tests.
- Market data outage tests.
- Android lifecycle and battery tests.
- Automation false-click and unknown-screen tests.
- Target app UI change detection tests.
- Audit log completeness.

## Phase 8: Production Launch

- Staging pilot.
- Limited production beta.
- Monitor prediction accuracy.
- Monitor support issues.
- Add kill switches for signal emission and settlement.
- Add kill switches for Android automation profiles.

## Future Work

- Withdrawal flow with admin review.
- More symbols.
- More periods.
- Improved ML model after enough live data.
- Official Binance Event Contract API integration if it becomes available.
- Institutional dashboard.
