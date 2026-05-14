param(
    [Parameter(Mandatory = $true)]
    [string]$AdminUrl,

    [string]$DatabaseName = "event_signal_prod",
    [string]$AppUser = "event_signal_app",
    [Parameter(Mandatory = $true)]
    [string]$AppPassword,

    [string]$PsqlPath = "C:\Program Files\PostgreSQL\18\bin\psql.exe"
)

$ErrorActionPreference = "Stop"

if (!(Test-Path -LiteralPath $PsqlPath)) {
    throw "psql not found at $PsqlPath"
}

function Invoke-Psql {
    param(
        [string]$DatabaseUrl,
        [string]$Sql
    )
    & $PsqlPath $DatabaseUrl -v ON_ERROR_STOP=1 -c $Sql
}

$safeDb = $DatabaseName.Replace('"', '""')
$safeUser = $AppUser.Replace('"', '""')
$safePassword = $AppPassword.Replace("'", "''")

Invoke-Psql -DatabaseUrl $AdminUrl -Sql "create database `"$safeDb`";" 2>$null
if ($LASTEXITCODE -ne 0) {
    Write-Host "database may already exist: $DatabaseName"
}

Invoke-Psql -DatabaseUrl $AdminUrl -Sql @"
do `$`$
begin
  if not exists (select from pg_roles where rolname = '$safeUser') then
    create role "$safeUser" login password '$safePassword';
  else
    alter role "$safeUser" login password '$safePassword';
  end if;
end
`$`$;
"@

$targetUrl = $AdminUrl -replace "/[^/?]+(\?.*)?$", "/$DatabaseName`$1"
Invoke-Psql -DatabaseUrl $targetUrl -Sql "grant connect on database `"$safeDb`" to `"$safeUser`";"
Invoke-Psql -DatabaseUrl $targetUrl -Sql "grant usage, create on schema public to `"$safeUser`";"
Invoke-Psql -DatabaseUrl $targetUrl -Sql "alter default privileges in schema public grant select, insert, update, delete on tables to `"$safeUser`";"
Invoke-Psql -DatabaseUrl $targetUrl -Sql "alter default privileges in schema public grant usage, select on sequences to `"$safeUser`";"

Write-Host "database ready: $DatabaseName"
Write-Host "app user ready: $AppUser"
