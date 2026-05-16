alter table prediction_settings
  add column if not exists calibration_lookback_minutes integer not null default 180,
  add column if not exists calibration_min_samples integer not null default 30,
  add column if not exists calibration_min_win_rate numeric(8,6) not null default 0.65,
  add column if not exists calibration_min_ev_usdt numeric(12,4) not null default 0.01;

update prediction_settings
set calibration_lookback_minutes = coalesce(calibration_lookback_minutes, 180),
    calibration_min_samples = coalesce(calibration_min_samples, 30),
    calibration_min_win_rate = coalesce(calibration_min_win_rate, 0.65),
    calibration_min_ev_usdt = coalesce(calibration_min_ev_usdt, 0.01)
where id = true;
