alter table auth_sessions
  add column if not exists app_session_key text;

update auth_sessions
set app_session_key = concat('legacy-app-key-', replace(access_token, 'access-', ''))
where app_session_key is null;

alter table auth_sessions
  alter column app_session_key set not null;
