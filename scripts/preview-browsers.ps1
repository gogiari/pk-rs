param([switch]$Foreground)

$ErrorActionPreference = 'Stop'
$previewRoot = Split-Path -Parent $PSScriptRoot
$previewBinary = Join-Path $previewRoot 'target/release/pk.exe'
$previewConfigDir = Join-Path $previewRoot 'target/browser-preview'
$previewConfigFile = Join-Path $previewConfigDir 'config.toml'
if (-not (Test-Path -LiteralPath $previewBinary)) {
    throw '먼저 npm --prefix web run build 및 cargo build --release --bin pk를 실행하세요.'
}
New-Item -ItemType Directory -Path $previewConfigDir -Force | Out-Null
if (-not (Test-Path -LiteralPath $previewConfigFile)) {
    $initialPreviewConfig = @'
web_port = 18253
http_port = 13128
socks_port = 11080
auto_connect = false
auto_open_browser = false
'@
    [System.IO.File]::WriteAllText($previewConfigFile, $initialPreviewConfig, [System.Text.UTF8Encoding]::new($false))
}
$previousConfigDir = $env:PK_CONFIG_DIR
try {
    $env:PK_CONFIG_DIR = $previewConfigDir
    if ($Foreground) { & $previewBinary start --foreground }
    else { & $previewBinary ui }
    if ($LASTEXITCODE -ne 0) { throw "로컬 미리보기 실행 실패: $LASTEXITCODE" }
} finally {
    $env:PK_CONFIG_DIR = $previousConfigDir
}
