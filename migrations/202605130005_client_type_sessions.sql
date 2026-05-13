alter table auth_sessions
  add column if not exists client_type text not null default 'android';

alter table auth_sessions
  drop constraint if exists auth_sessions_client_type_allowed;

alter table auth_sessions
  add constraint auth_sessions_client_type_allowed
  check (client_type in ('android', 'dapp'));

create index if not exists auth_sessions_user_client_idx on auth_sessions(user_id, client_type);
