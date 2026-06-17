param(
    [string]$Version = $env:VERSION,
    [string]$Target = $env:TARGET,
    [string]$BinaryPath = $env:BINARY_PATH,
    [string]$DistDir = $env:DIST_DIR
)

$ErrorActionPreference = "Stop"

if (-not $Version) {
    $Version = (Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
}
if (-not $Target) {
    $Target = "x86_64-pc-windows-msvc"
}
if (-not $BinaryPath) {
    $BinaryPath = "target\$Target\release\pile.exe"
}
if (-not $DistDir) {
    $DistDir = "dist"
}

if (-not (Test-Path $BinaryPath)) {
    throw "package-windows: binary not found: $BinaryPath"
}

if ($env:WINDOWS_SIGNTOOL_CERT_SHA1) {
    signtool sign `
        /fd SHA256 `
        /td SHA256 `
        /tr "http://timestamp.digicert.com" `
        /sha1 "$env:WINDOWS_SIGNTOOL_CERT_SHA1" `
        "$BinaryPath"
}

$packageName = "pile-$Version-$Target-windows"
$packageDir = Join-Path $DistDir $packageName
$zipPath = Join-Path $DistDir "$packageName.zip"

Remove-Item -Recurse -Force $packageDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $packageDir | Out-Null
Copy-Item $BinaryPath (Join-Path $packageDir "pile.exe")
Copy-Item LICENSE (Join-Path $packageDir "LICENSE")
Copy-Item README.md (Join-Path $packageDir "README.md")

Remove-Item -Force $zipPath -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $packageDir "*") -DestinationPath $zipPath
Write-Host "package-windows: wrote $zipPath"
