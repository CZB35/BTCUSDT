alter table event_contract_orders
  add column if not exists entry_price numeric(38,18);
