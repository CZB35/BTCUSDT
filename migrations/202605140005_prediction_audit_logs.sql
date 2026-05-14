create table if not exists prediction_audit_logs (
  id uuid primary key,
  signal_id uuid not null references signals(id) on delete cascade,
  source text not null,
  symbol text not null,
  period_seconds integer not null,
  model_version text not null,
  direction text not null,
  confidence numeric(8,6) not null,
  predicted_return numeric(18,10) not null,
  entry_price numeric(38,18) not null,
  suggested_stake numeric(38,18),
  score numeric(18,10),
  feature_snapshot jsonb not null default '{}'::jsonb,
  rule_hits jsonb not null default '[]'::jsonb,
  reason text,
  created_at timestamptz not null default now()
);

create index if not exists prediction_audit_logs_signal_idx on prediction_audit_logs(signal_id);
create index if not exists prediction_audit_logs_created_idx on prediction_audit_logs(created_at desc);
create index if not exists prediction_audit_logs_model_created_idx on prediction_audit_logs(model_version, created_at desc);
