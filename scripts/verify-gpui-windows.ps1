param(
    [Parameter(Mandatory = $true)][string]$BinaryPath,
    [Parameter(Mandatory = $true)][string]$ExpectedVersion
)

$ErrorActionPreference = 'Stop'
$taskBinary = (Resolve-Path -LiteralPath $BinaryPath).Path
$taskStream = [System.IO.File]::OpenRead($taskBinary)
$taskReader = [System.IO.BinaryReader]::new($taskStream)
try {
    if ($taskReader.ReadUInt16() -ne 0x5a4d) { throw 'Missing DOS executable header' }
    $taskStream.Position = 0x3c
    $taskPeOffset = $taskReader.ReadUInt32()
    if ($taskPeOffset + 96 -gt $taskStream.Length) { throw 'Truncated PE headers' }
    $taskStream.Position = $taskPeOffset
    if ($taskReader.ReadUInt32() -ne 0x4550) { throw 'Missing PE signature' }
    if ($taskReader.ReadUInt16() -ne 0x8664) { throw 'Expected a Windows x64 executable' }
    $taskStream.Position = $taskPeOffset + 24
    if ($taskReader.ReadUInt16() -ne 0x20b) { throw 'Expected a PE32+ executable' }
    $taskStream.Position = $taskPeOffset + 24 + 68
    if ($taskReader.ReadUInt16() -ne 2) {
        throw 'Expected Windows GUI subsystem; this may be a debug/console build'
    }
} finally {
    $taskReader.Dispose()
    $taskStream.Dispose()
}

$taskVersion = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($taskBinary)
foreach ($taskProperty in @('FileVersion', 'ProductVersion')) {
    if ($taskVersion.$taskProperty -ne $ExpectedVersion) {
        throw "$taskProperty differs from expected version $ExpectedVersion"
    }
}
foreach ($taskProperty in @('FileDescription', 'ProductName', 'InternalName')) {
    if ($taskVersion.$taskProperty -ne 'TP') { throw "$taskProperty must be TP" }
}
if ($taskVersion.OriginalFilename -ne 'tp-gpui.exe') { throw 'Unexpected embedded original filename' }

[pscustomobject]@{
    Path = $taskBinary
    Version = $taskVersion.ProductVersion
    Architecture = 'x64'
    Subsystem = 'Windows GUI'
    Bytes = (Get-Item -LiteralPath $taskBinary).Length
    SHA256 = (Get-FileHash -LiteralPath $taskBinary -Algorithm SHA256).Hash
}
