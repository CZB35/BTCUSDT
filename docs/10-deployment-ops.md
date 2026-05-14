# Deployment And Operations

## Environments

```text
local
staging
production
```

Each environment must have separate:

- Database.
- Redis.
- JWT secrets.
- BSC RPC provider keys.
- BSC vault contract and settlement wallet.
- FCM project.

## Production Services

```text
api-server
market-data-worker
prediction-worker
signal-worker
notification-worker
bsc-deposit-watcher
settlement-worker
admin-api
```

## Deployment Options

Recommended first release:

- Docker Compose for staging.
- Kubernetes or systemd-managed Docker for production.
- PostgreSQL managed service if budget allows.
- Redis managed service if budget allows.

## Docker Production Runbook

Create a production environment file from the template and keep it off Git:

```powershell
Copy-Item .env.production.example .env.production
```

Create a dedicated database and least-privilege app role:

```powershell
.\scripts\create-production-database.ps1 `
  -AdminUrl "postgres://admin:password@host:5432/postgres" `
  -DatabaseName event_signal_prod `
  -AppUser event_signal_app `
  -AppPassword "change_me"
```

Run migrations and start the API container:

```powershell
.\scripts\run-production-migrations.ps1
docker compose --env-file .env.production -f docker-compose.production.yml up -d api
```

The same image can run one-off migration mode with `event-signal-prototype --migrate-only`. The API serves `/v1`, `/admin`, and `/dapp` from one container for the first production cut.

## Observability

Metrics:

- API request latency.
- API error rate.
- WebSocket active connections.
- Signal creation count.
- Signal delivery latency.
- Market data lag.
- Prediction result accuracy.
- Deposit scan lag.
- Deposit credit failures.
- Ledger transaction failures.

Logs:

- Structured JSON logs.
- Include request ID.
- Include user ID only where necessary.
- Never log secrets, tokens, signatures, or private keys.

Alerts:

- Market data stale for > 2 minutes.
- BSC watcher lag > 20 blocks.
- Deposit credit failures > threshold.
- Settlement failures > threshold.
- WebSocket gateway restart loop.
- Database connection pool exhaustion.

## Security

- HTTPS only.
- JWT access tokens short-lived.
- Refresh token rotation.
- Passwords hashed with Argon2id.
- Admin users are stored in `admin_users`; the default super admin is seeded by database migration and should be changed after first login.
- Set admin frontend `VITE_API_BASE_URL` at build/dev-server time; do not expose backend address as a runtime login field.
- Set Android `EVENT_SIGNAL_API_BASE_URL` at build time for the target environment; default debug builds use `http://10.0.2.2:8787`.
- Set `BSC_RPC_URL`, `BSC_USDT_CONTRACT`, `BSC_VAULT_CONTRACT`, `BSC_SETTLEMENT_WALLET`, and `BSC_MIN_CONFIRMATIONS` before enabling automatic deposit crediting.
- Set `CORS_ALLOWED_ORIGINS` to comma-separated trusted admin/app origins; leave unset only for non-browser local API use.
- Strict rate limits on auth, wallet binding, tx submission.
- Admin endpoints behind separate auth and IP allowlist.
- Principle of least privilege for database users.

## Backup

- PostgreSQL PITR if possible.
- Daily logical backup.
- Test restore procedure monthly.
- Ledger tables are critical and must never be truncated.

## Chain Watcher Reliability

- Store last scanned block.
- Re-scan with overlap to handle reorgs.
- Treat recent deposits as confirming until minimum confirmation depth.
- Do not credit unconfirmed deposits.

## Market Data Reliability

- Store last successful source.
- Track source latency and error rate.
- Fail closed: no signal if data is stale.
- Use closed candles only.
