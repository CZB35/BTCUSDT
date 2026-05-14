create table if not exists prediction_observations (
  id uuid primary key,
  source text not null,
  symbol text not null,
  period_seconds integer not null,
  model_version text not null,
  direction text not null,
  confidence numeric(8,6) not null,
  predicted_return numeric(18,10) not null,
  entry_price numeric(38,18) not null,
  score numeric(18,10),
  feature_snapshot jsonb not null default '{}'::jsonb,
  reason text,
  created_at timestamptz not null default now()
);

create index if not exists prediction_observations_created_idx on prediction_observations(created_at desc);
create index if not exists prediction_observations_model_created_idx on prediction_observations(model_version, created_at desc);
