create table if not exists signal_outcomes (
  signal_id uuid primary key references signals(id) on delete cascade,
  symbol text not null,
  period_seconds integer not null,
  cycle_start timestamptz not null,
  cycle_end timestamptz not null,
  direction text not null,
  entry_price numeric(38,18) not null,
  expiry_price numeric(38,18) not null,
  result text not null,
  source text not null default 'prediction_observation',
  model_version text not null,
  settled_at timestamptz not null default now(),
  created_at timestamptz not null default now()
);

create index if not exists signal_outcomes_created_idx on signal_outcomes(created_at desc);
create index if not exists signal_outcomes_model_created_idx on signal_outcomes(model_version, created_at desc);
