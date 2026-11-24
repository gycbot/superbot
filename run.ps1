# Runs botd on Windows. Loads .env into this process, then starts the server.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$envFile = Join-Path $root ".env"
$exe = Join-Path $root "target\release\botd.exe"

if (-not (Test-Path $envFile)) {
    Write-Host "Missing .env. Copy .env.example to .env and fill it in." -ForegroundColor Yellow
    exit 1
}
if (-not (Test-Path $exe)) {
    Write-Host "Missing target\release\botd.exe. Build it first: cargo build --release" -ForegroundColor Yellow
    exit 1
}

Get-Content $envFile | ForEach-Object {
    $line = $_.Trim()
    if ($line -eq "" -or $line.StartsWith("#")) { return }
    $i = $line.IndexOf("=")
    if ($i -lt 1) { return }
    $key = $line.Substring(0, $i).Trim()
    $value = $line.Substring($i + 1).Trim()
    if ($value.Length -ge 2 -and $value.StartsWith('"') -and $value.EndsWith('"')) {
        $value = $value.Substring(1, $value.Length - 2)
    }
    [Environment]::SetEnvironmentVariable($key, $value, "Process")
}

Push-Location $root
try {
    & $exe
} finally {
    Pop-Location
}
