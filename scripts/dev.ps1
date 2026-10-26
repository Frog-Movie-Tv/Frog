# DEVELOPMENT ONLY. Docker is how Frog actually runs; this is for working on it.
#
#   .\scripts\dev.ps1            build, seed two demo accounts, run on :8080
#   .\scripts\dev.ps1 -Fresh     throw the dev database away first
#   .\scripts\dev.ps1 -NoSeed    skip seeding
#
# Everything lives in data\dev.db, which is gitignored and disposable.

param(
	[switch]$Fresh,
	[switch]$NoSeed
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

if (-not (Test-Path (Join-Path $repo '.env'))) {
	Copy-Item (Join-Path $repo '.env.example') (Join-Path $repo '.env')
	Write-Host 'Created .env from .env.example. Put your free TMDB key in it.' -ForegroundColor Yellow
}

Write-Host '==> backend' -ForegroundColor Cyan
Push-Location (Join-Path $repo 'backend')
cargo build
if ($LASTEXITCODE -ne 0) { Pop-Location; exit 1 }
Pop-Location

Write-Host '==> frontend' -ForegroundColor Cyan
Push-Location (Join-Path $repo 'frontend')
if (-not (Test-Path 'node_modules')) { npm ci }
npm run build
if ($LASTEXITCODE -ne 0) { Pop-Location; exit 1 }
Pop-Location

$db = Join-Path $repo 'data\dev.db'
if (-not $NoSeed) {
	if ($Fresh -or -not (Test-Path $db)) {
		Write-Host '==> seeding two demo accounts' -ForegroundColor Cyan
		if ($Fresh) { node scripts/seed-demo.mjs --force } else { node scripts/seed-demo.mjs }
		if ($LASTEXITCODE -ne 0) { exit 1 }
	}
	else {
		Write-Host "==> data\dev.db exists; leaving it alone (-Fresh to replace)" -ForegroundColor DarkGray
	}
}

Write-Host '==> http://127.0.0.1:8080  (Ctrl+C to stop)' -ForegroundColor Green
$env:DATABASE_URL = 'sqlite:./data/dev.db?mode=rwc'
$env:FROG_BIND = '127.0.0.1:8080'
$env:FROG_STATIC_DIR = './frontend/build'
if (-not $env:RUST_LOG) { $env:RUST_LOG = 'frog=debug,tower_http=warn' }
& (Join-Path $repo 'backend\target\debug\frog.exe')
