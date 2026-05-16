# Prediction Engine

## Objective

Predict BTCUSDT return over the next 10 minutes and output one of:

```text
UP
DOWN
HOLD
```

with:

- Predicted return.
- Confidence.
- Suggested stake.
- Model version.

## Data Sources

Primary available source observed in this environment:

```text
https://data-api.binance.vision/api/v3/klines?symbol=BTCUSDT&interval=1m&limit=...
```

Production should support multiple sources:

- Binance data API.
- Binance spot REST if available.
- Binance futures REST if available.
- Optional fallback exchange feeds for liveness checks only.

## Candle Rules

- Use only closed 1-minute candles.
- Deduplicate by open time.
- Validate continuity.
- Backfill missing candles before generating signal.
- Mark market data stale if latest closed candle is older than 90 seconds.

## Baseline Features

For each prediction timestamp, compute:

- Return over 1, 3, 5, 10, 15, 30 minutes.
- EMA 5/10/20 slope.
- RSI 6/14.
- MACD fast/slow/histogram.
- Bollinger band position and width.
- Short-term realized volatility.
- Volume z-score.
- Taker buy ratio if available.
- Candle body-to-range ratio.
- Recent high/low breakout distance.

## First Model

Use a transparent weighted score model first. The first pass is a continuation score:

```text
continuation_score = momentum_score
                   + trend_score
                   + volume_score
                   + mean_reversion_score
                   + candle_score
                   + live_score
                   + cycle_score
                   + basis_score
```

At Bollinger-band extremes the engine applies an event-contract guard:

- If price is already extended in the same direction, emit `HOLD` instead of chasing a late move.
- If price reaches an extreme and then confirms a reclaim/rejection with live returns, use a separate top/bottom reversal score.
- Reversal score must still pass the same configured thresholds before a real signal is emitted.

Direction mapping:

```text
score >= up_threshold -> UP
score <= down_threshold -> DOWN
otherwise -> HOLD
```

Confidence:

```text
confidence = sigmoid(abs(score)) adjusted by volatility and data quality
```

## Signal Filters

Emit `HOLD` if:

- Market data is stale.
- Volatility is too low for payout economics.
- Volatility is too high and direction confidence is weak.
- Spread/depth data is missing when required.
- Confidence is below configured threshold.
- The last N real-time predictions are underperforming.

## Suggested Stake

Do not use aggressive martingale logic in first version.

Suggested stake follows the user-saved percentage rule:

```text
if available_balance < 5 USDT:
  suggested_stake = null
else:
  suggested_stake = max(available_balance * user_stake_percent, 5 USDT)
  suggested_stake = min(suggested_stake, user_single_order_limit, 250 USDT)
```

Default `user_stake_percent` is 5%. First-release allowed range is 1% to 20%.

Optional confidence multiplier can be added later, but first release should keep the formula simple and auditable.

Examples:

```text
available balance 100 USDT, percent 5% -> suggested stake 5 USDT
available balance 200 USDT, percent 5% -> suggested stake 10 USDT
available balance 500 USDT, percent 5% -> suggested stake 25 USDT
available balance 500 USDT, percent 10% -> suggested stake 50 USDT
available balance 1000 USDT, percent 5% -> suggested stake 50 USDT
```

## Backtesting

Required metrics:

- Direction accuracy.
- UP accuracy.
- DOWN accuracy.
- HOLD frequency.
- Average predicted return vs realized return.
- Maximum losing streak.
- Rolling 24h and 7d performance.
- Performance by volatility regime.

## Live Monitoring

Every emitted signal should be evaluated after 10 minutes:

```text
entry_price = signal.entry_price
exit_price = candle close at cycle_end or nearest official reference price
realized_return = exit_price / entry_price - 1
result = direction matched realized_return sign
```

Store this in `prediction_results` for model monitoring.
