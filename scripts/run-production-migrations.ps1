param(
    [string]$ComposeFile = "docker-compose.production.yml"
)

$ErrorActionPreference = "Stop"

if (!(Test-Path -LiteralPath ".env.production")) {
    throw ".env.production is missing. Copy .env.production.example and fill production values first."
}

docker compose --env-file .env.production -f $ComposeFile build api
docker compose --env-file .env.production -f $ComposeFile --profile tools run --rm migrate
