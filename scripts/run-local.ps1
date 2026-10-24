<#
.SYNOPSIS
    Runs Frog directly on Windows, without Docker.

.DESCRIPTION
    Builds the frontend if it is missing, builds the Rust binary if it is
    missing, then starts the server with paths that make sense outside a
    container. Your .env is still read for TMDB_API_KEY and the rest; the two
    container-specific paths (DATABASE_URL and FROG_STATIC_DIR) are overridden
    here, because ./static and /data only exist inside the image.

.EXAMPLE
    .\scripts\run-local.ps1
    .\scripts\run-local.ps1 -Port 9000
    .\scripts\run-local.ps1 -Rebuild
#>
[CmdletBinding()]
param(
    [int]$Port = 8080,
    [switch]$Rebuild
)

$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
$cargo = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargo) { $env:PATH = "$cargo;$env:PATH" }

function Need($name, $hint) {
    if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
        throw "$name is not on PATH. $hint"
    }
}

Need 'cargo' 'Install Rust from https://rustup.rs and reopen the terminal.'
Need 'npm'   'Install Node.js 22 or newer from https://nodejs.org.'

# --- frontend ------------------------------------------------------------
$static = Join-Path $repo 'frontend\build'
if ($Rebuild -or -not (Test-Path (Join-Path $static 'index.html'))) {
    Write-Host 'Building the web app...' -ForegroundColor Cyan
    Push-Location (Join-Path $repo 'frontend')
    try {
        if (-not (Test-Path 'node_modules')) { npm ci }
        npm run build
    } finally { Pop-Location }
}

# --- backend -------------------------------------------------------------
$binary = Join-Path $repo 'backend\target\release\frog.exe'
if ($Rebuild -or -not (Test-Path $binary)) {
    Write-Host 'Building the server (first time takes a few minutes)...' -ForegroundColor Cyan
    Push-Location (Join-Path $repo 'backend')
    try { cargo build --release --locked } finally { Pop-Location }
}

# --- run -----------------------------------------------------------------
$data = Join-Path $repo 'data'
New-Item -ItemType Directory -Force -Path $data | Out-Null

$dbPath = (Join-Path $data 'frog.db') -replace '\\', '/'
$env:DATABASE_URL = "sqlite:$dbPath" + '?mode=rwc'
$env:FROG_STATIC_DIR = $static
$env:FROG_BIND = "127.0.0.1:$Port"

# Warn early rather than letting the UI be the one to tell you.
$envFile = Join-Path $repo '.env'
if (Test-Path $envFile) {
    $key = (Get-Content $envFile | Where-Object { $_ -match '^\s*TMDB_API_KEY\s*=\s*\S' })
    if (-not $key) {
        Write-Host 'TMDB_API_KEY is empty in .env - search and artwork will not work.' -ForegroundColor Yellow
        Write-Host 'Get a free key at https://www.themoviedb.org/settings/api' -ForegroundColor Yellow
    }
} else {
    Write-Host '.env is missing. Run: Copy-Item .env.example .env' -ForegroundColor Yellow
}

Write-Host ''
Write-Host "Frog is starting on http://127.0.0.1:$Port" -ForegroundColor Green
Write-Host "Database: $dbPath" -ForegroundColor DarkGray
Write-Host 'Press Ctrl+C to stop.' -ForegroundColor DarkGray
Write-Host ''

& $binary
