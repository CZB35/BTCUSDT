update prediction_settings
set interval_seconds = 1,
    up_threshold = 8.00,
    down_threshold = -8.00,
    min_confidence = 0.72,
    updated_at = now()
where id = true
  and interval_seconds = 5
  and up_threshold = 2.60
  and down_threshold = -2.60
  and min_confidence = 0.62;
