param(
    [Parameter(Mandatory = $true)][string]$BinaryPath,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [Parameter(Mandatory = $true)][string]$NoticesPath,
    [string]$RuntimeDirectory,
    [string]$DumpbinPath,
    [Parameter(Mandatory = $true)][string]$ExpectedVersion
)

# Assemble installer input only. Never runs or installs the application.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/find-gpui-windows-tools.ps1"
$taskTools = Find-GpuiWindowsTools $RuntimeDirectory $DumpbinPath
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskInstallerIdentity = Get-Content -LiteralPath (Join-Path $taskRoot 'packaging/windows/identity.json') -Raw | ConvertFrom-Json
if ($taskInstallerIdentity.productName -ne 'TP' -or $taskInstallerIdentity.executable -ne 'TP.exe') {
    throw 'Windows installer identity requires compatibility review'
}
$taskBinary = (Resolve-Path -LiteralPath $BinaryPath).Path
$taskNotices = (Resolve-Path -LiteralPath $NoticesPath).Path
$taskRuntime = (Resolve-Path -LiteralPath $taskTools.RuntimeDirectory).Path
$taskDumpbin = (Resolve-Path -LiteralPath $taskTools.DumpbinPath).Path
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $taskOutput) { throw 'Output directory already exists' }
if ([string]::IsNullOrWhiteSpace([IO.File]::ReadAllText($taskNotices))) { throw 'Third-party notices must not be empty' }
$taskIdentity = & "$PSScriptRoot/verify-gpui-windows.ps1" -BinaryPath $taskBinary -ExpectedVersion $ExpectedVersion

function Get-TaskImports([string]$Path) {
    $taskLines = & $taskDumpbin /dependents $Path 2>&1
    if ($LASTEXITCODE -ne 0) { throw "Dependency inspection failed: $Path" }
    $taskNames = @($taskLines | ForEach-Object {
        if ($_ -match '^\s+([a-zA-Z0-9_.-]+\.dll)\s*$') { $Matches[1].ToLowerInvariant() }
    } | Sort-Object -Unique)
    if (-not $taskNames.Count) { throw "No DLL imports found: $Path" }
    return $taskNames
}

function Test-TaskX64([string]$Path) {
    $taskReader = [IO.BinaryReader]::new([IO.File]::OpenRead($Path))
    try {
        if ($taskReader.ReadUInt16() -ne 0x5a4d) { throw "Invalid DLL header: $Path" }
        $taskReader.BaseStream.Position = 0x3c
        $taskOffset = $taskReader.ReadUInt32()
        $taskReader.BaseStream.Position = $taskOffset
        if ($taskReader.ReadUInt32() -ne 0x4550 -or $taskReader.ReadUInt16() -ne 0x8664) {
            throw "Expected an x64 runtime DLL: $Path"
        }
    } finally { $taskReader.Dispose() }
}

# Windows 11 system dependencies. Unknown libraries require an explicit review;
# do not silently resolve them from the developer machine's System32 or PATH.
$taskSystem = @('kernel32.dll', 'kernelbase.dll', 'bcrypt.dll', 'bcryptprimitives.dll',
    'advapi32.dll', 'user32.dll', 'ole32.dll', 'shell32.dll', 'oleaut32.dll', 'gdi32.dll',
    'comctl32.dll', 'version.dll', 'ntdll.dll', 'dwmapi.dll', 'dxgi.dll', 'd3d11.dll',
    'dwrite.dll', 'icuuc.dll', 'dcomp.dll', 'imm32.dll', 'ucrtbase.dll', 'msvcrt.dll')
$taskPending = [Collections.Generic.Queue[string]]::new()
$taskPending.Enqueue($taskBinary)
$taskRuntimeFiles = @{}
$taskDependencies = @{}
while ($taskPending.Count) {
    $taskCurrent = $taskPending.Dequeue()
    $taskImports = @(Get-TaskImports $taskCurrent)
    $taskDependencies[[IO.Path]::GetFileName($taskCurrent)] = $taskImports
    foreach ($taskImport in $taskImports) {
        if ($taskImport -in $taskSystem -or $taskImport -like 'api-ms-win-*.dll' -or $taskImport -like 'ext-ms-win-*.dll') { continue }
        if ($taskImport -notmatch '^(vcruntime140(_1)?|msvcp140(_[a-z0-9_]+)?|concrt140)\.dll$') {
            throw "Unreviewed non-system dependency: $taskImport"
        }
        if ($taskRuntimeFiles.ContainsKey($taskImport)) { continue }
        $taskSource = Join-Path $taskRuntime $taskImport
        if (-not (Test-Path -LiteralPath $taskSource -PathType Leaf)) { throw "Required runtime DLL missing: $taskSource" }
        Test-TaskX64 $taskSource
        $taskSignature = Get-AuthenticodeSignature -LiteralPath $taskSource
        if ($taskSignature.Status -ne 'Valid' -or $taskSignature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') {
            throw "Expected a valid Microsoft-signed redistributable DLL: $taskSource"
        }
        $taskRuntimeFiles[$taskImport] = $taskSource
        $taskPending.Enqueue($taskSource)
    }
}

# Validate everything before creating a new staging directory. On copy failure,
# leave it for inspection; never recursively delete a caller-supplied location.
[void][IO.Directory]::CreateDirectory($taskOutput)
[IO.File]::Copy($taskBinary, (Join-Path $taskOutput $taskInstallerIdentity.executable))
[IO.File]::Copy($taskNotices, (Join-Path $taskOutput 'third-party-notices.txt'))
[IO.File]::Copy((Join-Path $taskRoot 'LICENSE'), (Join-Path $taskOutput 'LICENSE'))
foreach ($taskName in $taskRuntimeFiles.Keys) {
    [IO.File]::Copy($taskRuntimeFiles[$taskName], (Join-Path $taskOutput $taskName))
}
$taskFiles = @(Get-ChildItem -LiteralPath $taskOutput -File | Sort-Object Name | ForEach-Object {
    [ordered]@{ name = $_.Name; bytes = $_.Length; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
})
$taskReport = [ordered]@{
    product = $taskInstallerIdentity.productName; version = $taskIdentity.Version; architecture = 'x64'; executable = $taskInstallerIdentity.executable
    installerIdentity = $taskInstallerIdentity
    dependencies = $taskDependencies; runtimeSources = $taskRuntimeFiles; files = $taskFiles
}
$taskReport | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $taskOutput 'package-manifest.json') -Encoding utf8
Write-Output "Staged GPUI Windows files: $taskOutput"
