alter table prediction_settings
  add column if not exists technical_weight numeric(8,6) not null default 0.35,
  add column if not exists probability_weight numeric(8,6) not null default 0.65,
  add column if not exists volatility_floor numeric(12,8) not null default 0.00016;

update prediction_settings
set technical_weight = coalesce(technical_weight, 0.35),
    probability_weight = coalesce(probability_weight, 0.65),
    volatility_floor = coalesce(volatility_floor, 0.00016)
where id = true;
