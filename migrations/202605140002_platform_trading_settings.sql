create table if not exists platform_trading_settings (
  id boolean primary key default true,
  stake_percent numeric(8,6) not null default 0.05,
  min_stake_usdt numeric(38,18) not null default 5,
  max_single_stake_usdt numeric(38,18) not null default 250,
  updated_at timestamptz not null default now(),
  constraint platform_trading_settings_singleton check (id = true),
  constraint platform_stake_percent_range check (stake_percent >= 0.01 and stake_percent <= 0.20),
  constraint platform_min_stake_range check (min_stake_usdt >= 5 and min_stake_usdt <= 250),
  constraint platform_max_stake_range check (max_single_stake_usdt >= 5 and max_single_stake_usdt <= 250),
  constraint platform_stake_bounds check (min_stake_usdt <= max_single_stake_usdt)
);

insert into platform_trading_settings
  (id, stake_percent, min_stake_usdt, max_single_stake_usdt)
values
  (true, 0.05, 5, 250)
on conflict (id) do nothing;
