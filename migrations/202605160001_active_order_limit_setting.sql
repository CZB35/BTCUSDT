alter table platform_trading_settings
  add column if not exists max_active_event_orders_per_user integer not null default 3;

alter table platform_trading_settings
  drop constraint if exists platform_max_active_event_orders_range;

alter table platform_trading_settings
  add constraint platform_max_active_event_orders_range
  check (max_active_event_orders_per_user >= 1 and max_active_event_orders_per_user <= 20);

