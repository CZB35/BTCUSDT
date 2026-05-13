create table if not exists admin_users (
  id uuid primary key,
  username text not null unique,
  password_hash text not null,
  role text not null default 'super_admin',
  status text not null default 'active',
  last_login_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint admin_users_role_allowed check (role in ('super_admin', 'admin')),
  constraint admin_users_status_allowed check (status in ('active', 'disabled'))
);

create table if not exists admin_sessions (
  access_token text primary key,
  admin_user_id uuid not null references admin_users(id) on delete cascade,
  expires_at timestamptz not null,
  created_at timestamptz not null default now()
);

create table if not exists admin_audit_logs (
  id uuid primary key,
  admin_user_id uuid references admin_users(id) on delete set null,
  action text not null,
  target_type text,
  target_id text,
  ip_address text,
  created_at timestamptz not null default now()
);

insert into admin_users (id, username, password_hash, role, status, created_at, updated_at)
values (
  '018f0000-0000-7000-8000-000000000001',
  '2752186242',
  '$argon2id$v=19$m=19456,t=2,p=1$Yg+Rws/AhevYWlikD3F7lg$2QqeBfA9pR0qOgiCrDuEYF0XKCWxZ0ViQ+yisx80XF4',
  'super_admin',
  'active',
  now(),
  now()
)
on conflict (username) do update set
  password_hash = excluded.password_hash,
  role = excluded.role,
  status = excluded.status,
  updated_at = now();

create index if not exists admin_sessions_user_id_idx on admin_sessions(admin_user_id);
create index if not exists admin_sessions_expires_at_idx on admin_sessions(expires_at);
create index if not exists admin_audit_logs_admin_created_idx on admin_audit_logs(admin_user_id, created_at desc);
