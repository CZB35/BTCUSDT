# Product Scope

## Goal

Build a production-grade Android and backend system that provides BTCUSDT 10-minute event contract signals, user recharge activation, signal permissions, execution report reconciliation, and per-order net-profit sharing.

The platform does not custody user Binance accounts and does not require Binance trading API permissions. Binance API keys, if supported later, must be read-only only.

## First Release Features

- Android login and device binding.
- Auth rate limiting on register only to reduce scripted bulk signups and server abuse.
- BSC wallet binding by signature.
- BSC BEP20 USDT deposit verification to one platform receiving address.
- User platform balance and activation status.
- BTCUSDT 1-minute K-line collection and 10-minute prediction.
- Signal delivery through WebSocket and push notification.
- Production automation is single-shot WebSocket driven; missed or lagged signals are skipped, not replayed through latest-signal polling.
- Automatic execution uses pre-signal gates: Android must be online and recently report that it is on the Binance event contract page; backend balance must be sufficient for fee reserve.
- Floating Android window showing signal, countdown, confidence, and suggested amount.
- Android automation module for user-authorized event contract operation based on delivered signals and saved risk settings.
- Suggested amount rule: user-configurable percentage of available platform balance, default 5%, clamped to the event-contract order range of 5-250 USDT.
- Signal authorization freezes the estimated 30% winning-profit share before Android receives an automatic trade signal, so later manual withdrawals cannot remove fee coverage.
- User trading preference settings: stake percentage, single-order cap, minimum signal confidence, and enable/disable signal alerts.
- Automation preference settings: enable/disable automation, execution mode, target app profile, per-order confirmation requirement, and emergency stop.
- Copy helpers for signal direction and suggested amount.
- Execution report submission from Android automation and manual reconciliation flows.
- Settlement: winning orders deduct 30% of verified gross profit; losing orders credit 30% of verified loss amount back to platform balance.
- Admin dashboard API foundation for user, deposit, signal, and ledger inspection.

## Explicit Non-Goals For First Release

- No automatic withdrawal function. Withdrawals, if added, require manual approval and must use `available` balance only.
- No cross-platform iOS app.
- No direct Binance Event Contract API integration unless official endpoints are obtained.
- No promise of profit or guaranteed signal accuracy.

## Business Rules

- Users must recharge USDT before seeing full signals or participating in profit-share settlement.
- Users must log in before recharge. Deposit credit belongs to the platform account, not to a transient wallet session.
- Users must bind a BSC wallet address before recharge crediting.
- A deposit is valid only if the chain transaction matches the bound wallet address and platform receiving address.
- One account may have only one active device session at a time. A second device login is rejected until the active session logs out.
- Winning reports charge 30% of verified positive gross profit from the pre-frozen reserve when available.
- Losing reports release any pre-frozen reserve and credit 30% of the verified loss amount back to the user's platform balance. If no negative gross profit is reported, the loss amount defaults to `stake_usdt`.
- Draw reports release any pre-frozen reserve without fee or rebate.
- All balance changes must be recorded through immutable ledger entries.
- If available balance is below 5 USDT, the user is not eligible for a tradeable suggested stake.
- If available balance is 5 USDT or above, suggested stake is `max(available_balance * user_stake_percent, 5)` capped at 250 USDT and any stricter user risk limit.
- Automatic mode must execute only backend-authorized signals with a `reservation_id`; local execution shortcuts are not part of the production path.
- If the page gate is missing, stale, or not ready, the backend must not freeze balance or send a trade-authorized signal.
- `/v1/signals/latest` and history endpoints are display-only and must never authorize or compensate automatic orders.
- The default stake percentage is 5%.
- The configurable stake percentage range for first release is 1% to 20%.
- Saved settings are stored on the backend and cached locally on Android for display only.
- Automation must use backend-returned signal direction, suggested stake, confidence threshold, and user risk limits.
- Every automation attempt must generate an execution report with signal ID, amount, direction, result status, timestamps, and enough audit detail for settlement review.
- Users must be able to pause automation immediately from the floating window or notification.

## Important Product Assumptions

- Target users use Chinese UI.
- Target chain is BSC.
- Target asset is BEP20 USDT.
- Target symbol is BTCUSDT.
- Target prediction cycle is 10 minutes.
- First release is Android-only.
