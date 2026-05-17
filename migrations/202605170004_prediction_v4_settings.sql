update prediction_settings
set
  up_threshold = 8.00,
  down_threshold = -8.00,
  min_confidence = greatest(min_confidence, 0.62),
  technical_weight = 0.35,
  probability_weight = 0.65,
  volatility_floor = greatest(volatility_floor, 0.00016),
  calibration_lookback_minutes = greatest(calibration_lookback_minutes, 180),
  calibration_min_samples = greatest(calibration_min_samples, 30),
  calibration_min_win_rate = greatest(calibration_min_win_rate, 0.60),
  calibration_min_ev_usdt = greatest(calibration_min_ev_usdt, 0.01),
  updated_at = now()
where id = true;
