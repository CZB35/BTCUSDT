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
- Platform receiving address.
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
- Set `ADMIN_TOKEN` before enabling admin endpoints.
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
