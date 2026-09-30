Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$projectRoot = $PSScriptRoot
$webRoot = Join-Path $projectRoot 'web'
$nodeModules = Join-Path $webRoot 'node_modules'
$dependencyStamp = Join-Path $nodeModules '.pk-deps-sha256'
$frontendStamp = Join-Path $nodeModules '.pk-frontend-inputs'
$packageFile = Join-Path $webRoot 'package.json'
$lockFile = Join-Path $webRoot 'package-lock.json'

if (-not (Test-Path -LiteralPath $lockFile)) {
    throw 'web/package-lock.json is missing. Run npm install in web first.'
}

$npm = Get-Command npm.cmd -ErrorAction SilentlyContinue
if ($null -eq $npm) {
    $npm = Get-Command npm -ErrorAction Stop
}
$cargo = Get-Command cargo -ErrorAction Stop

$packageHash = (Get-FileHash -LiteralPath $packageFile -Algorithm SHA256).Hash
$lockHash = (Get-FileHash -LiteralPath $lockFile -Algorithm SHA256).Hash
$dependencyHash = "$packageHash`:$lockHash"
$installedHash = if (Test-Path -LiteralPath $dependencyStamp) {
    (Get-Content -LiteralPath $dependencyStamp -Raw).Trim()
} else {
    ''
}

Push-Location $webRoot
try {
    if (-not (Test-Path -LiteralPath $nodeModules) -or $installedHash -ne $dependencyHash) {
        Write-Host 'Installing frontend dependencies...'
        & $npm.Source ci
        if ($LASTEXITCODE -ne 0) { throw "npm ci failed with exit code $LASTEXITCODE" }
        Set-Content -LiteralPath $dependencyStamp -Value $dependencyHash -NoNewline
    } else {
        Write-Host 'Frontend dependencies are up to date.'
    }

    $sourcePaths = @(
        $packageFile
        $lockFile
        (Join-Path $webRoot 'index.html')
        (Join-Path $webRoot 'vite.config.ts')
        (Join-Path $webRoot 'tsconfig.json')
    )
    $sourcePaths += @(Get-ChildItem -LiteralPath (Join-Path $webRoot 'src') -Recurse -File |
        Sort-Object FullName | ForEach-Object FullName)
    $frontendInputs = ($sourcePaths | ForEach-Object {
        '{0}:{1}' -f $_.Substring($webRoot.Length), (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash
    }) -join "`n"
    $builtInputs = if (Test-Path -LiteralPath $frontendStamp) {
        (Get-Content -LiteralPath $frontendStamp -Raw).Trim()
    } else {
        ''
    }

    if (-not (Test-Path -LiteralPath (Join-Path $webRoot 'dist/index.html')) -or
        -not (Test-Path -LiteralPath (Join-Path $webRoot 'dist/assets')) -or
        $frontendInputs -ne $builtInputs) {
        Write-Host 'Building React frontend...'
        & $npm.Source run build
        if ($LASTEXITCODE -ne 0) { throw "Frontend build failed with exit code $LASTEXITCODE" }
        Set-Content -LiteralPath $frontendStamp -Value $frontendInputs -NoNewline
    } else {
        Write-Host 'React frontend is up to date.'
    }
} finally {
    Pop-Location
}

Push-Location $projectRoot
try {
    Write-Host 'Building Rust release binary...'
    & $cargo.Source build --release --bin pk
    if ($LASTEXITCODE -ne 0) { throw "Rust build failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}

Write-Host "Build complete: $(Join-Path $projectRoot 'target/release/pk.exe')"
if ($env:OS -eq 'Windows_NT') {
    $npmBinaryDir = Join-Path $projectRoot 'npm/binaries/win32-x64'
    New-Item -ItemType Directory -Force -Path $npmBinaryDir | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target/release/pk.exe') -Destination (Join-Path $npmBinaryDir 'pk.exe')
    Write-Host "npm executable: $(Join-Path $npmBinaryDir 'pk.exe')"
}
