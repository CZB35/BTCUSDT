create table if not exists user_exchange_credentials (
  user_id uuid primary key references users(id) on delete cascade,
  provider text not null default 'binance_futures',
  api_key text not null,
  api_secret_ciphertext text not null,
  can_read_account boolean not null default false,
  verification_status text not null default 'pending',
  balance_available_usdt numeric(38,18),
  balance_wallet_usdt numeric(38,18),
  balance_updated_at timestamptz,
  last_verified_at timestamptz,
  last_error text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint user_exchange_provider_allowed check (provider in ('binance_futures'))
);

create table if not exists binance_income_events (
  id uuid primary key,
  user_id uuid not null references users(id) on delete cascade,
  provider text not null default 'binance_futures',
  external_ref text not null,
  income_type text not null,
  income numeric(38,18) not null,
  asset text not null,
  event_time timestamptz not null,
  tran_id text,
  trade_id text,
  raw jsonb,
  created_at timestamptz not null default now(),
  unique(user_id, provider, external_ref, income_type, income, event_time)
);

alter table event_contract_orders
  add column if not exists exchange_ref text,
  add column if not exists exchange_order_income_id uuid references binance_income_events(id),
  add column if not exists exchange_order_amount numeric(38,18),
  add column if not exists exchange_order_time timestamptz,
  add column if not exists exchange_verification_status text not null default 'unverified',
  add column if not exists exchange_verified_at timestamptz;

alter table execution_reports
  add column if not exists exchange_ref text,
  add column if not exists exchange_payout_income_id uuid references binance_income_events(id),
  add column if not exists exchange_verified_at timestamptz;

create index if not exists user_exchange_credentials_provider_idx
  on user_exchange_credentials(provider, verification_status);

create index if not exists binance_income_events_user_ref_idx
  on binance_income_events(user_id, external_ref, income_type, event_time desc);

create index if not exists event_contract_orders_exchange_pending_idx
  on event_contract_orders(exchange_verification_status, created_at)
  where exchange_verification_status in ('unverified', 'pending', 'confirmed');
