create table if not exists prediction_settings (
  id boolean primary key default true,
  enabled boolean not null default true,
  interval_seconds integer not null default 1,
  up_threshold numeric(18,10) not null default 8.00,
  down_threshold numeric(18,10) not null default -8.00,
  min_confidence numeric(8,6) not null default 0.72,
  updated_at timestamptz not null default now(),
  constraint prediction_settings_singleton check (id = true)
);

insert into prediction_settings (id, enabled, interval_seconds, up_threshold, down_threshold, min_confidence)
values (true, true, 1, 8.00, -8.00, 0.72)
on conflict (id) do nothing;
