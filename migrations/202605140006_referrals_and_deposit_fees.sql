alter table deposits
  add column credited_amount numeric(38,18) not null default 0,
  add column platform_fee numeric(38,18) not null default 0;

alter table platform_trading_settings
  add column first_deposit_min_usdt numeric(38,18) not null default 100,
  add column deposit_fee_rate numeric(8,6) not null default 0.25,
  add column direct_referral_rate numeric(8,6) not null default 0.09,
  add column indirect_referral_rate numeric(8,6) not null default 0.03,
  add constraint platform_first_deposit_min_positive check (first_deposit_min_usdt >= 10),
  add constraint platform_deposit_fee_rate_range check (deposit_fee_rate >= 0 and deposit_fee_rate <= 0.50),
  add constraint platform_direct_referral_rate_range check (direct_referral_rate >= 0 and direct_referral_rate <= 0.50),
  add constraint platform_indirect_referral_rate_range check (indirect_referral_rate >= 0 and indirect_referral_rate <= 0.50);

create table referral_links (
  user_id uuid primary key references users(id) on delete cascade,
  parent_user_id uuid references users(id) on delete set null,
  grandparent_user_id uuid references users(id) on delete set null,
  referral_code text not null unique,
  invited_by_code text,
  created_at timestamptz not null default now(),
  constraint referral_no_self_parent check (parent_user_id is null or parent_user_id <> user_id),
  constraint referral_no_self_grandparent check (grandparent_user_id is null or grandparent_user_id <> user_id)
);

create table referral_rewards (
  id uuid primary key,
  settlement_id uuid not null references settlements(id) on delete cascade,
  source_user_id uuid not null references users(id) on delete cascade,
  recipient_user_id uuid not null references users(id) on delete cascade,
  level integer not null,
  rate numeric(8,6) not null,
  base_profit numeric(38,18) not null,
  amount numeric(38,18) not null,
  status text not null default 'credited',
  created_at timestamptz not null default now(),
  unique(settlement_id, recipient_user_id, level),
  constraint referral_reward_level_allowed check (level in (1, 2)),
  constraint referral_reward_positive check (amount > 0)
);

create index referral_links_parent_idx on referral_links(parent_user_id);
create index referral_links_grandparent_idx on referral_links(grandparent_user_id);
create index referral_rewards_recipient_created_idx on referral_rewards(recipient_user_id, created_at desc);
