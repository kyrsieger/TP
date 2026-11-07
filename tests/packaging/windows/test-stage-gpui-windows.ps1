param(
    [Parameter(Mandatory = $true)][string]$BinaryPath,
    [Parameter(Mandatory = $true)][string]$ExpectedVersion,
    [string]$RuntimeDirectory,
    [string]$DumpbinPath
)

$ErrorActionPreference = 'Stop'
if (-not $RuntimeDirectory -or -not $DumpbinPath) {
    $taskVswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $taskVs = & $taskVswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $taskVs) { throw 'Visual Studio C++ tools were not found' }
    if (-not $DumpbinPath) {
        $taskTools = Get-ChildItem -LiteralPath (Join-Path $taskVs 'VC/Tools/MSVC') -Directory |
            Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
        $DumpbinPath = Join-Path $taskTools.FullName 'bin/Hostx64/x64/dumpbin.exe'
    }
    if (-not $RuntimeDirectory) {
        $taskRedist = Get-ChildItem -LiteralPath (Join-Path $taskVs 'VC/Redist/MSVC') -Directory |
            Where-Object { $_.Name -match '^\d+\.\d+\.\d+(\.\d+)?$' } |
            Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
        $RuntimeDirectory = Join-Path $taskRedist.FullName 'x64/Microsoft.VC143.CRT'
    }
}
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$taskScripts = Join-Path $taskRoot 'scripts'
$taskWork = Join-Path $taskRoot "target/gpui-stage-checks-$([guid]::NewGuid().ToString('N'))"
[void][IO.Directory]::CreateDirectory($taskWork)
$taskNotices = Join-Path $taskWork 'TEST-NOTICES.txt'
[IO.File]::WriteAllText($taskNotices, 'TEST FIXTURE ONLY. Incomplete notices. Do not distribute.')
$taskBase = @{
    BinaryPath = (Resolve-Path -LiteralPath $BinaryPath).Path
    ExpectedVersion = $ExpectedVersion
    NoticesPath = $taskNotices
    RuntimeDirectory = (Resolve-Path -LiteralPath $RuntimeDirectory).Path
    DumpbinPath = (Resolve-Path -LiteralPath $DumpbinPath).Path
}
$taskStageScript = Join-Path $taskScripts 'stage-gpui-windows.ps1'
$taskGood = Join-Path $taskWork 'valid'
& $taskStageScript @taskBase -OutputDirectory $taskGood
$taskManifest = Get-Content -LiteralPath (Join-Path $taskGood 'package-manifest.json') -Raw | ConvertFrom-Json
if ($taskManifest.executable -ne 'TP.exe' -or $taskManifest.version -ne $ExpectedVersion) { throw 'Wrong staged identity' }
foreach ($taskFile in $taskManifest.files) {
    $taskStagedFile = Join-Path $taskGood $taskFile.name
    if ((Get-Item -LiteralPath $taskStagedFile).Length -ne $taskFile.bytes -or
        (Get-FileHash -LiteralPath $taskStagedFile -Algorithm SHA256).Hash -ne $taskFile.sha256) {
        throw "Invalid staged file: $($taskFile.name)"
    }
}
if ((Get-FileHash -LiteralPath (Join-Path $taskGood 'TP.exe')).Hash -ne (Get-FileHash -LiteralPath $taskBase.BinaryPath).Hash) {
    throw 'Executable changed during staging'
}

function Assert-TaskRejected([string]$Name, [hashtable]$Overrides, [string]$Message) {
    $taskArguments = $taskBase.Clone()
    $taskDestination = Join-Path $taskWork $Name
    $taskArguments.OutputDirectory = $taskDestination
    foreach ($taskKey in $Overrides.Keys) { $taskArguments[$taskKey] = $Overrides[$taskKey] }
    $taskRejected = $false
    try { & $taskStageScript @taskArguments } catch {
        if ($_.Exception.Message -notlike "$Message*") { throw }
        $taskRejected = $true
    }
    if (-not $taskRejected) { throw "Invalid input accepted: $Name" }
    if ($Name -ne 'valid' -and (Test-Path -LiteralPath $taskDestination)) { throw "Failure created output: $Name" }
    Write-Output "Rejected as expected: $Name"
}

Assert-TaskRejected 'valid' @{} 'Output directory already exists'
Assert-TaskRejected 'version' @{ ExpectedVersion = '0.0.0-test-mismatch' } 'FileVersion differs'
$taskEmpty = Join-Path $taskWork 'empty-notices.txt'
[IO.File]::WriteAllText($taskEmpty, ' ')
Assert-TaskRejected 'empty-notices' @{ NoticesPath = $taskEmpty } 'Third-party notices must not be empty'
Assert-TaskRejected 'missing-runtime' @{ RuntimeDirectory = $taskWork } 'Required runtime DLL missing'

# Modify only a copy of the import name. Never execute any fixture binary.
$taskBytes = [IO.File]::ReadAllBytes($taskBase.BinaryPath)
$taskNeedle = [Text.Encoding]::ASCII.GetBytes('kernel32.dll')
$taskReplacement = [Text.Encoding]::ASCII.GetBytes('tp_probe.dll')
$taskMatches = 0
for ($taskIndex = 0; $taskIndex -le $taskBytes.Length - $taskNeedle.Length; $taskIndex++) {
    if ($taskBytes[$taskIndex] -ne $taskNeedle[0]) { continue }
    $taskMatch = $true
    for ($taskOffset = 1; $taskOffset -lt $taskNeedle.Length; $taskOffset++) {
        if ($taskBytes[$taskIndex + $taskOffset] -ne $taskNeedle[$taskOffset]) { $taskMatch = $false; break }
    }
    if ($taskMatch) { [Array]::Copy($taskReplacement, 0, $taskBytes, $taskIndex, $taskReplacement.Length); $taskMatches++ }
}
if (-not $taskMatches) { throw 'Import mutation fixture could not be constructed' }
$taskUnknown = Join-Path $taskWork 'unknown-import.exe'
[IO.File]::WriteAllBytes($taskUnknown, $taskBytes)
Assert-TaskRejected 'unknown-import' @{ BinaryPath = $taskUnknown } 'Unreviewed non-system dependency: tp_probe.dll'

$taskCorruptDir = Join-Path $taskWork 'corrupt-runtime-input'
[void][IO.Directory]::CreateDirectory($taskCorruptDir)
$taskRuntimeBytes = [IO.File]::ReadAllBytes((Join-Path $taskBase.RuntimeDirectory 'vcruntime140.dll'))
$taskRuntimeBytes[0x400] = $taskRuntimeBytes[0x400] -bxor 1
[IO.File]::WriteAllBytes((Join-Path $taskCorruptDir 'vcruntime140.dll'), $taskRuntimeBytes)
Assert-TaskRejected 'corrupt-runtime' @{ RuntimeDirectory = $taskCorruptDir } 'Expected a valid Microsoft-signed redistributable DLL'
Write-Output "Windows staging checks passed. Test-only artifacts retained at: $taskWork"
