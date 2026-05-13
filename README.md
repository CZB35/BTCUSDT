# BTCUSDT Event Signal Platform

This repository contains the production development documentation for an Android-first BTCUSDT 10-minute event signal platform.

The product scope is:

- Android app built with Kotlin and Jetpack Compose.
- Floating signal window for users who keep Binance Event Contracts open.
- Floating signal window calculates suggested stake from the user's saved stake percentage, defaulting to 5% of platform balance with a 10 USDT minimum when balance allows it.
- Rust backend for market data, prediction, user balance, BSC USDT deposit tracking, signal delivery, and profit-share accounting.
- BSC BEP20 USDT recharge using one platform receiving address plus user wallet ownership verification.
- First release does not include withdrawals.
- Android automation module for user-authorized event contract execution, audit logging, and settlement reconciliation.
- Execution reports connect Android automation results back to backend settlement and profit-share accounting.

## Documents

- [Product Scope](docs/01-product-scope.md)
- [System Architecture](docs/02-system-architecture.md)
- [Android App](docs/03-android-app.md)
- [Rust Backend](docs/04-rust-backend.md)
- [Database Schema](docs/05-database-schema.md)
- [BSC USDT Deposits](docs/06-bsc-usdt-deposits.md)
- [Ledger And Profit Share](docs/07-ledger-profit-share.md)
- [Prediction Engine](docs/08-prediction-engine.md)
- [API Contract](docs/09-api-contract.md)
- [Deployment And Operations](docs/10-deployment-ops.md)
- [Roadmap](docs/11-roadmap.md)

## Technology Decision

Use Jetpack Compose instead of Flutter for the first version because this product is Android-only and depends heavily on native Android capabilities: overlay windows, foreground services, notifications, permissions, app lifecycle, and vendor battery policies.

Use Rust for the backend because the platform needs high-concurrency signal delivery, async workers, chain event processing, strict accounting, and reliable long-running services.
