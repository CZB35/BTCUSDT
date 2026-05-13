create table if not exists users (
  id uuid primary key,
  wallet_address text unique not null,
  password_hash text not null,
  status text not null default 'active',
  active_device_fingerprint text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table if not exists auth_sessions (
  access_token text primary key,
  user_id uuid not null references users(id) on delete cascade,
  device_fingerprint text not null,
  refresh_token text not null unique,
  expires_at timestamptz not null,
  created_at timestamptz not null default now()
);

create table if not exists auth_rate_limits (
  id uuid primary key,
  scope text not null,
  key text not null,
  window_start timestamptz not null,
  attempts integer not null default 0,
  blocked_until timestamptz,
  updated_at timestamptz not null default now(),
  unique(scope, key)
);

create table if not exists wallet_bindings (
  id uuid primary key,
  user_id uuid not null references users(id) on delete cascade,
  chain text not null default 'bsc',
  address text not null,
  verified boolean not null default false,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique(chain, address)
);

create table if not exists balances (
  user_id uuid primary key references users(id) on delete cascade,
  asset text not null default 'USDT',
  available numeric(38,18) not null default 0,
  frozen numeric(38,18) not null default 0,
  updated_at timestamptz not null default now()
);

create table if not exists user_trading_settings (
  user_id uuid primary key references users(id) on delete cascade,
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

create table if not exists user_automation_settings (
  user_id uuid primary key references users(id) on delete cascade,
  automation_enabled boolean not null default false,
  execution_mode text not null default 'manual',
  target_app_profile text,
  require_final_confirmation boolean not null default true,
  emergency_stop_enabled boolean not null default true,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint execution_mode_allowed check (execution_mode in ('manual', 'assisted', 'automatic'))
);

create table if not exists automation_page_states (
  user_id uuid primary key references users(id) on delete cascade,
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

create table if not exists ledger_entries (
  id uuid primary key,
  user_id uuid not null references users(id) on delete cascade,
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

create table if not exists deposits (
  id uuid primary key,
  user_id uuid not null references users(id) on delete cascade,
  chain text not null default 'bsc',
  asset text not null default 'USDT',
  token_contract text not null default '',
  tx_hash text not null unique,
  from_address text not null default '',
  to_address text not null default '',
  amount numeric(38,18) not null,
  block_number bigint not null default 0,
  confirmations integer not null default 0,
  status text not null,
  credited_at timestamptz,
  created_at timestamptz not null default now()
);

create table if not exists signals (
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

create table if not exists signal_reservations (
  id uuid primary key,
  user_id uuid not null references users(id) on delete cascade,
  signal_id uuid not null references signals(id) on delete cascade,
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

create table if not exists event_contract_orders (
  id uuid primary key,
  user_id uuid not null references users(id) on delete cascade,
  reservation_id uuid references signal_reservations(id),
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

create table if not exists execution_reports (
  id uuid primary key,
  user_id uuid not null references users(id) on delete cascade,
  signal_id uuid not null references signals(id) on delete cascade,
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

create table if not exists settlements (
  id uuid primary key,
  execution_report_id uuid not null unique references execution_reports(id) on delete cascade,
  user_id uuid not null references users(id) on delete cascade,
  gross_profit numeric(38,18) not null,
  profit_share_fee numeric(38,18) not null,
  loss_rebate numeric(38,18) not null default 0,
  user_net_profit numeric(38,18) not null,
  status text not null,
  created_at timestamptz not null default now()
);

create index if not exists auth_sessions_user_id_idx on auth_sessions(user_id);
create index if not exists wallet_bindings_user_chain_idx on wallet_bindings(user_id, chain);
create index if not exists ledger_entries_user_created_idx on ledger_entries(user_id, created_at desc);
create index if not exists event_contract_orders_user_created_idx on event_contract_orders(user_id, created_at desc);
create index if not exists execution_reports_user_created_idx on execution_reports(user_id, created_at desc);
create index if not exists settlements_user_created_idx on settlements(user_id, created_at desc);
