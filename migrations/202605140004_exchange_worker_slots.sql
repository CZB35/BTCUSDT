create table if not exists exchange_worker_slots (
    id uuid primary key,
    name text not null,
    shard_total integer not null check (shard_total >= 1),
    shard_index integer not null check (shard_index >= 0),
    bind_local_ip text,
    enabled boolean not null default false,
    status text not null default 'stopped',
    last_started_at timestamptz,
    last_stopped_at timestamptz,
    last_error text,
    updated_at timestamptz not null default now(),
    created_at timestamptz not null default now(),
    check (shard_index < shard_total)
);

create unique index if not exists exchange_worker_slots_shard_unique
    on exchange_worker_slots (shard_total, shard_index);
