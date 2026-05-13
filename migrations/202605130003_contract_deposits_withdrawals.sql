create table if not exists withdrawals (
  id uuid primary key,
  user_id uuid not null references users(id) on delete cascade,
  chain text not null default 'bsc',
  asset text not null default 'USDT',
  token_contract text not null default '',
  contract_address text not null default '',
  tx_hash text unique,
  refund_id text not null unique,
  payout_wallet text,
  to_address text not null,
  amount numeric(38,18) not null,
  block_number bigint,
  confirmations integer not null default 0,
  status text not null,
  requested_at timestamptz not null default now(),
  processed_at timestamptz,
  rejected_at timestamptz,
  reject_reason text,
  constraint withdrawals_status_allowed check (
    status in ('pending_review', 'paid', 'confirming', 'rejected')
  )
);

alter table deposits
  add column if not exists contract_address text not null default '';

alter table deposits
  add column if not exists deposit_id text not null default '';

create index if not exists withdrawals_user_requested_idx on withdrawals(user_id, requested_at desc);
create index if not exists withdrawals_status_requested_idx on withdrawals(status, requested_at desc);
create unique index if not exists deposits_deposit_id_unique_idx
  on deposits(deposit_id)
  where deposit_id <> '';
