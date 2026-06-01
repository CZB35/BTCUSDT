update event_contract_orders
set entry_price = null
where source = 'android_automation'
  and entry_price is not null
  and raw_audit ? 'observed_index_price';

update execution_reports
set entry_price = null
where source in ('android_automation', 'binance_income')
  and entry_price is not null;
