alter table automation_page_states
  add column if not exists observed_balance_usdt numeric(38,18),
  add column if not exists account_match_status text not null default 'unknown',
  add column if not exists account_balance_delta_usdt numeric(38,18);

create index if not exists automation_page_states_account_match_idx
  on automation_page_states(account_match_status, updated_at desc);
