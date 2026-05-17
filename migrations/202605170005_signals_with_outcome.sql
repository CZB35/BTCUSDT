create or replace view signals_with_outcome as
select
  s.id,
  s.symbol,
  s.period_seconds,
  s.cycle_start,
  s.cycle_end,
  s.direction,
  s.confidence,
  s.predicted_return,
  s.entry_price,
  s.suggested_stake,
  s.model_version,
  s.created_at,
  o.result as outcome_result,
  o.expiry_price as outcome_expiry_price,
  o.settled_at as outcome_settled_at
from signals s
left join signal_outcomes o on o.signal_id = s.id;
