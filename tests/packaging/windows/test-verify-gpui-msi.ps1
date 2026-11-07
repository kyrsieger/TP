param(
    [Parameter(Mandatory = $true)][string]$MsiPath,
    [Parameter(Mandatory = $true)][string]$StageDirectory,
    [Parameter(Mandatory = $true)][string]$WixTools
)

$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$taskScripts = Join-Path $taskRoot 'scripts'
$taskStage = (Resolve-Path -LiteralPath $StageDirectory).Path
$taskDark = Join-Path $WixTools 'dark.exe'
& "$taskScripts/verify-gpui-msi.ps1" -MsiPath $MsiPath -StageDirectory $taskStage -DarkPath $taskDark
$taskFixture = Join-Path $taskRoot ('target/gpui-msi-hash-check-' + [guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $taskFixture)
$taskChangedStage = Join-Path $taskFixture 'stage'
Copy-Item -LiteralPath $taskStage -Destination $taskChangedStage -Recurse
$taskLicense = Join-Path $taskChangedStage 'LICENSE'
$taskBytes = [IO.File]::ReadAllBytes($taskLicense)
if ($taskBytes.Length -eq 0) { throw 'Expected a nonempty license fixture' }
$taskBytes[0] = $taskBytes[0] -bxor 1
[IO.File]::WriteAllBytes($taskLicense, $taskBytes)
$taskManifestPath = Join-Path $taskChangedStage 'package-manifest.json'
$taskManifest = Get-Content -LiteralPath $taskManifestPath -Raw | ConvertFrom-Json
($taskManifest.files | Where-Object name -eq 'LICENSE').sha256 = (Get-FileHash -LiteralPath $taskLicense -Algorithm SHA256).Hash
$taskManifest | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $taskManifestPath -Encoding utf8
$taskOutput = Join-Path $taskFixture 'msi'
python "$taskScripts/package_gpui_windows_msi.py" --stage $taskChangedStage --output $taskOutput --wix-tools $WixTools
if ($LASTEXITCODE -ne 0) { throw 'Failed to compile changed-payload MSI fixture' }
$taskMsi = @(Get-ChildItem -LiteralPath $taskOutput -Filter '*.msi')
if ($taskMsi.Count -ne 1) { throw 'Expected exactly one MSI fixture' }
$taskRejected = $false
try {
    & "$taskScripts/verify-gpui-msi.ps1" -MsiPath $taskMsi[0].FullName -StageDirectory $taskStage -DarkPath $taskDark
} catch {
    if ($_.Exception.Message -notlike '*MSI payload hash mismatch: LICENSE*') { throw }
    $taskRejected = $true
}
if (-not $taskRejected) { throw 'Same-size changed MSI payload was accepted' }
$taskBytes[0] = $taskBytes[0] -bxor 1
[IO.File]::WriteAllBytes($taskLicense, $taskBytes)
$taskRejected = $false
try {
    & "$taskScripts/verify-gpui-msi.ps1" -MsiPath $MsiPath -StageDirectory $taskChangedStage -DarkPath $taskDark
} catch {
    if ($_.Exception.Message -notlike '*Staged payload hash mismatch: LICENSE*') { throw }
    $taskRejected = $true
}
if (-not $taskRejected) { throw 'Changed staging file with stale manifest was accepted' }
Write-Output "PASS: unchanged payloads accepted; same-size changed MSI and stale staging hashes rejected. Evidence: $taskFixture"
