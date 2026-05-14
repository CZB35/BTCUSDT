create table if not exists signal_deliveries (
  id uuid primary key,
  signal_id uuid not null references signals(id) on delete cascade,
  user_id uuid not null references users(id) on delete cascade,
  reservation_id uuid references signal_reservations(id) on delete set null,
  ws_connection_id uuid not null,
  delivery_status text not null default 'sent',
  authorization_status text not null,
  trade_authorized boolean not null default false,
  stake_usdt numeric(38,18) not null default 0,
  fee_reserve_usdt numeric(38,18) not null default 0,
  sent_at timestamptz not null,
  received_at timestamptz,
  client_received_at timestamptz,
  page_state jsonb,
  app_version text,
  last_error text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique(signal_id, user_id, ws_connection_id),
  constraint signal_delivery_status_allowed check (
    delivery_status in ('sent', 'received', 'ack_duplicate', 'failed')
  )
);

create index if not exists signal_deliveries_signal_user_idx
  on signal_deliveries(signal_id, user_id, sent_at desc);

create index if not exists signal_deliveries_status_idx
  on signal_deliveries(delivery_status, sent_at desc);
