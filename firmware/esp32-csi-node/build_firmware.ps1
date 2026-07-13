[CmdletBinding()]
param(
    [string]$IdfPath = $env:IDF_PATH,
    [string]$PythonPath,
    [string]$Port = $env:ESP32_PORT,
    [switch]$Flash,
    [switch]$SkipClean
)

$ErrorActionPreference = 'Stop'

# MSYS variables make ESP-IDF reject otherwise valid Windows toolchains.
'MSYSTEM', 'MSYSTEM_CARCH', 'MSYSTEM_CHOST', 'MSYSTEM_PREFIX',
'MINGW_CHOST', 'MINGW_PACKAGE_PREFIX', 'MINGW_PREFIX' | ForEach-Object {
    Remove-Item "env:$_" -ErrorAction SilentlyContinue
}

if (-not $IdfPath) {
    throw 'ESP-IDF is not configured. Open an ESP-IDF PowerShell or pass -IdfPath C:\path\to\esp-idf.'
}

$IdfPath = (Resolve-Path -LiteralPath $IdfPath).Path
$exportScript = Join-Path $IdfPath 'export.ps1'
$idf = Join-Path $IdfPath 'tools\idf.py'
if (-not (Test-Path -LiteralPath $idf -PathType Leaf)) {
    throw "idf.py was not found under $IdfPath"
}

# Populate the matching compiler, CMake, Ninja, and Python environment.
if (Test-Path -LiteralPath $exportScript -PathType Leaf) {
    . $exportScript
}

if (-not $PythonPath -and $env:IDF_PYTHON_ENV_PATH) {
    $candidate = Join-Path $env:IDF_PYTHON_ENV_PATH 'Scripts\python.exe'
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
        $PythonPath = $candidate
    }
}
if (-not $PythonPath) {
    $PythonPath = (Get-Command python -ErrorAction Stop).Source
}

Push-Location $PSScriptRoot
try {
    if (-not $SkipClean) {
        Write-Host '=== Cleaning stale build cache ==='
        & $PythonPath $idf fullclean
        if ($LASTEXITCODE -ne 0) { throw "ESP-IDF fullclean failed with exit code $LASTEXITCODE" }
    }

    Write-Host '=== Building ESP32 CSI firmware ==='
    & $PythonPath $idf build
    if ($LASTEXITCODE -ne 0) { throw "ESP-IDF build failed with exit code $LASTEXITCODE" }

    if ($Flash) {
        if (-not $Port) {
            throw 'Flashing requires -Port COMx or the ESP32_PORT environment variable.'
        }
        Write-Host "=== Build succeeded; flashing to $Port ==="
        & $PythonPath $idf -p $Port flash
        if ($LASTEXITCODE -ne 0) { throw "ESP-IDF flash failed with exit code $LASTEXITCODE" }
    } else {
        Write-Host '=== Build succeeded (flash skipped; pass -Flash -Port COMx to flash) ==='
    }
}
finally {
    Pop-Location
}
