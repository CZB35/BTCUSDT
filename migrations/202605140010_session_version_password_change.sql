alter table users
  add column if not exists session_version integer not null default 0;

alter table auth_sessions
  add column if not exists session_version integer not null default 0;

create index if not exists auth_sessions_user_version_idx
  on auth_sessions(user_id, session_version);
