# Database Schema

Use PostgreSQL. Monetary fields must use `numeric(38, 18)` or Rust `Decimal`; never use floating point for balances.

## users

```sql
create table users (
  id uuid primary key,
  email text unique,
  password_hash text not null,
  status text not null default 'active',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);
```

## devices

```sql
create table devices (
  id uuid primary key,
  user_id uuid not null references users(id),
  platform text not null default 'android',
  device_fingerprint text not null,
  fcm_token text,
  last_seen_at timestamptz,
  created_at timestamptz not null default now(),
  unique(user_id, device_fingerprint)
);
```

## auth_rate_limits

Tracks register attempts per email and per IP. Blocked rows reject requests until `blocked_until`.

```sql
create table auth_rate_limits (
  id uuid primary key,
  scope text not null,
  key text not null,
  window_start timestamptz not null,
  attempts integer not null default 0,
  blocked_until timestamptz,
  updated_at timestamptz not null default now(),
  unique(scope, key)
);
```

## wallet_bindings

```sql
create table wallet_bindings (
  id uuid primary key,
  user_id uuid not null references users(id),
  chain text not null default 'bsc',
  address text not null,
  verified_at timestamptz not null,
  status text not null default 'active',
  created_at timestamptz not null default now(),
  unique(chain, address)
);
```

## balances

```sql
create table balances (
  user_id uuid primary key references users(id),
  asset text not null default 'USDT',
  available numeric(38,18) not null default 0,
  frozen numeric(38,18) not null default 0,
  updated_at timestamptz not null default now()
);
```

## user_trading_settings

```sql
create table user_trading_settings (
  user_id uuid primary key references users(id),
  stake_percent numeric(8,6) not null default 0.05,
  min_stake_usdt numeric(38,18) not null default 5,
  max_single_stake_usdt numeric(38,18),
  min_confidence numeric(8,6),
  signal_alert_enabled boolean not null default true,
  floating_window_enabled boolean not null default true,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint stake_percent_range check (stake_percent >= 0.01 and stake_percent <= 0.20),
  constraint min_stake_positive check (min_stake_usdt > 0),
  constraint max_single_stake_positive check (max_single_stake_usdt is null or max_single_stake_usdt > 0),
  constraint min_confidence_range check (min_confidence is null or (min_confidence >= 0 and min_confidence <= 1))
);
```

The backend creates this row with defaults when a user is registered or when settings are first requested.

## user_automation_settings

```sql
create table user_automation_settings (
  user_id uuid primary key references users(id),
  automation_enabled boolean not null default false,
  execution_mode text not null default 'manual',
  target_app_profile text,
  require_final_confirmation boolean not null default true,
  emergency_stop_enabled boolean not null default true,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint execution_mode_allowed check (execution_mode in ('manual', 'assisted', 'automatic'))
);
```

The Android app reads this row before starting automation. Automatic mode must still obey signal freshness, confidence, balance, and per-order stake limits.

## automation_page_states

Latest Android page gate heartbeat. The backend reads this row before authorizing a WebSocket signal; stale rows close the gate.

```sql
create table automation_page_states (
  user_id uuid primary key references users(id),
  page_ready boolean not null default false,
  screen_name text not null default '',
  confidence numeric(8,6) not null default 0,
  package_name text not null default '',
  amount_field_found boolean not null default false,
  up_button_found boolean not null default false,
  down_button_found boolean not null default false,
  observed_at timestamptz not null,
  updated_at timestamptz not null default now()
);
```

## ledger_entries

Immutable ledger. Balance updates must reference ledger entries.

```sql
create table ledger_entries (
  id uuid primary key,
  user_id uuid not null references users(id),
  asset text not null default 'USDT',
  entry_type text not null,
  amount numeric(38,18) not null,
  balance_available_after numeric(38,18) not null,
  balance_frozen_after numeric(38,18) not null,
  ref_type text not null,
  ref_id uuid not null,
  idempotency_key text not null unique,
  created_at timestamptz not null default now()
);
```

Entry types:

```text
deposit_credit
signal_fee_reserve
signal_fee_reserve_release
reserved_profit_share_fee
profit_share_fee
loss_rebate
manual_adjustment
```

## deposits

```sql
create table deposits (
  id uuid primary key,
  user_id uuid not null references users(id),
  chain text not null default 'bsc',
  token_contract text not null,
  tx_hash text not null unique,
  from_address text not null,
  to_address text not null,
  amount numeric(38,18) not null,
  block_number bigint not null,
  confirmations integer not null default 0,
  status text not null,
  credited_at timestamptz,
  created_at timestamptz not null default now()
);
```

## candles_1m

```sql
create table candles_1m (
  symbol text not null,
  open_time timestamptz not null,
  close_time timestamptz not null,
  open numeric(38,18) not null,
  high numeric(38,18) not null,
  low numeric(38,18) not null,
  close numeric(38,18) not null,
  volume numeric(38,18) not null,
  quote_volume numeric(38,18),
  trade_count integer,
  taker_buy_base_volume numeric(38,18),
  taker_buy_quote_volume numeric(38,18),
  primary key(symbol, open_time)
);
```

## signals

```sql
create table signals (
  id uuid primary key,
  symbol text not null default 'BTCUSDT',
  period_seconds integer not null default 600,
  cycle_start timestamptz not null,
  cycle_end timestamptz not null,
  direction text not null,
  confidence numeric(8,6) not null,
  predicted_return numeric(18,10) not null,
  entry_price numeric(38,18) not null,
  suggested_stake numeric(38,18),
  model_version text not null,
  status text not null default 'active',
  created_at timestamptz not null default now(),
  unique(symbol, period_seconds, cycle_start)
);
```

Directions:

```text
UP
DOWN
HOLD
```

## execution_reports

```sql
create table execution_reports (
  id uuid primary key,
  user_id uuid not null references users(id),
  signal_id uuid not null references signals(id),
  client_report_id text not null,
  source text not null default 'manual',
  symbol text not null,
  period_seconds integer not null,
  direction text not null,
  stake_usdt numeric(38,18) not null,
  entry_price numeric(38,18),
  expiry_price numeric(38,18),
  payout_ratio numeric(18,10),
  gross_profit numeric(38,18),
  result text not null,
  automation_profile text,
  automation_confidence numeric(8,6),
  action_trace_id text,
  raw_audit jsonb,
  report_status text not null default 'pending',
  reported_at timestamptz not null,
  created_at timestamptz not null default now(),
  unique(user_id, client_report_id)
);
```

`source` should be `manual`, `android_assisted`, or `android_automation`. Store only audit metadata by default; keep screenshots or large traces in object storage and reference them from `raw_audit`.

## signal_reservations

Created before an automatic signal is delivered. It freezes estimated fee coverage so manual withdrawals cannot remove funds needed for settlement.

```sql
create table signal_reservations (
  id uuid primary key,
  user_id uuid not null references users(id),
  signal_id uuid not null references signals(id),
  stake_usdt numeric(38,18) not null,
  expected_profit_usdt numeric(38,18) not null,
  fee_reserve_usdt numeric(38,18) not null,
  status text not null default 'reserved',
  order_id uuid,
  execution_report_id uuid,
  expires_at timestamptz not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique(user_id, signal_id)
);
```

Status values: `reserved`, `order_submitted`, `settled`, `released`, `expired`.

## event_contract_orders

Records Android-confirmed order attempts immediately after the app clicks the final confirmation button.

```sql
create table event_contract_orders (
  id uuid primary key,
  user_id uuid not null references users(id),
  reservation_id uuid,
  signal_ref text not null,
  client_order_id text not null,
  source text not null default 'android_automation',
  symbol text not null,
  period_seconds integer not null,
  direction text not null,
  stake_usdt numeric(38,18) not null,
  payout_ratio numeric(18,10),
  expected_payout_usdt numeric(38,18),
  expected_profit_usdt numeric(38,18),
  automation_profile text,
  automation_confidence numeric(8,6),
  action_trace_id text,
  raw_audit jsonb,
  order_status text not null default 'submitted',
  reported_at timestamptz,
  created_at timestamptz not null default now(),
  unique(user_id, client_order_id)
);
```

## settlements

```sql
create table settlements (
  id uuid primary key,
  execution_report_id uuid not null unique references execution_reports(id),
  user_id uuid not null references users(id),
  gross_profit numeric(38,18) not null,
  profit_share_fee numeric(38,18) not null,
  loss_rebate numeric(38,18) not null default 0,
  user_net_profit numeric(38,18) not null,
  status text not null,
  created_at timestamptz not null default now()
);
```
