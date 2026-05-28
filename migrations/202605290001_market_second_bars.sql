create table if not exists market_second_bars (
  symbol text not null,
  second_ms bigint not null,
  high numeric(38,18) not null,
  low numeric(38,18) not null,
  close numeric(38,18) not null,
  volume numeric(38,18) not null default 0,
  taker_buy_volume numeric(38,18) not null default 0,
  taker_sell_volume numeric(38,18) not null default 0,
  trade_count integer not null default 0,
  price_from_index boolean not null default false,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (symbol, second_ms)
);

create index if not exists market_second_bars_symbol_second_desc_idx
  on market_second_bars(symbol, second_ms desc);
