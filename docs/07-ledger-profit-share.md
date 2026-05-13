# Ledger And Profit Share

## Accounting Principle

All balance changes must be represented by immutable ledger entries. Never update balances without writing a ledger entry in the same database transaction.

Use integer smallest-unit accounting internally if possible. If using decimals, use PostgreSQL `numeric(38,18)` and Rust `Decimal`.

## Balance Types

```text
available: user can use this balance for product permissions and fees
frozen: reserved for pending settlement or signal fee coverage
```

The platform does not freeze the Binance stake because execution happens in the Binance app. It freezes only the estimated platform fee reserve before delivering an automatic trade signal.

## Authorization Reserve

```text
stake = clamp(max(available * user_stake_percent, 5 USDT), 5, 250 USDT)
expected_profit = stake * EVENT_CONTRACT_PAYOUT_RATIO
fee_reserve = expected_profit * 0.30
```

Automatic mode receives a tradeable signal only if the page gate is fresh and `available >= fee_reserve`. The backend moves `fee_reserve` from `available` to `frozen` while building the WebSocket `signal.created` event and returns `reservation_id` with that event. Display/history endpoints do not create reservations.

## Settlement Rule

```text
WIN:  platform_fee = max(gross_profit, 0) * 0.30
LOSS: loss_rebate = loss_amount * 0.30
DRAW: no fee and no rebate
```

For losses, `loss_amount` is the absolute value of negative `gross_profit` when reported; otherwise it defaults to `stake_usdt`. Winning orders consume the frozen reserve first. Losing or draw reports release the frozen reserve before applying any loss rebate.

## Settlement Inputs

Settlement requires an execution report containing:

- User ID.
- Signal ID.
- Direction.
- Stake amount.
- Result.
- Gross profit.
- Optional payout ratio.
- Optional entry and expiry prices.
- Source, such as `manual`, `android_assisted`, or `android_automation`.
- Optional automation profile, confidence score, and action trace reference.

## Settlement Status

```text
pending
settled
rejected
review_required
```

## Ledger Example

User deposits 100 USDT:

```text
deposit_credit +100.00000000
available after: 100.00000000
```

User receives an authorized 5 USDT signal with 80% expected profit. The backend freezes 1.2 USDT:

```text
signal_fee_reserve -1.20000000
available after: 98.80000000
frozen after: 1.20000000
```

If the user wins with 4 USDT gross profit, platform fee is 1.2 USDT and the frozen reserve is consumed:

```text
reserved_profit_share_fee -1.20000000
frozen after: 0.00000000
```

If the user loses a 5 USDT stake, release the 1.2 USDT reserve and credit 1.5 USDT loss rebate:

```text
signal_fee_reserve_release +1.20000000
loss_rebate +1.50000000
```

## Fee Collection Models

### Prepaid Reserve Deduction

The user maintains platform balance. Expected profit-share fees are frozen before automatic signals and settled after reports.

Pros:

- Easy to enforce access.
- Manual withdrawals cannot drain fee coverage after a signal has been authorized.

Cons:

- Fee and rebate still depend on accurate order/result reporting.

### Subscription Or Signal Fee

The platform charges per signal or per day/month.

Pros:

- Easy to verify.
- No per-order profit dispute.

Cons:

- Different business model from net-profit sharing.

### Hybrid

Use prepaid access plus profit-share accounting. If execution report cannot be verified, do not auto-charge profit share; mark review required.

## Required Idempotency

Every money-changing endpoint must include or generate idempotency keys:

```text
deposit:{tx_hash}:{log_index}
settlement:{execution_report_id}
signal_fee_reserve:{reservation_id}
reserved_profit_share_fee:{execution_report_id}
signal_fee_reserve_release:{execution_report_id}
loss_rebate:{execution_report_id}
```

## Audit Requirements

- Store who/what created every execution report.
- Store raw execution report payload.
- Store Android automation source, profile version, confidence, and action trace reference when available.
- Store settlement calculation fields.
- Store model version that produced signal.
- Store all ledger entries permanently.
