param([Parameter(Mandatory = $true)][string]$DataDirectory)

# Resolve every prerequisite before launching. A failed fixture setup must never
# fall through to the user's normal TP data or startup registration.
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskData = (Resolve-Path -LiteralPath $DataDirectory).Path
$taskTarget = (Resolve-Path -LiteralPath (Join-Path $taskRoot 'target')).Path
if (-not $taskData.StartsWith($taskTarget + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Diagnostic data must be in a fixture directory beneath this workspace target directory'
}
if ((Split-Path -Leaf $taskData) -eq 'TP') { throw 'Diagnostic directory must not trigger the TP legacy migration' }
if (-not (Test-Path -LiteralPath (Join-Path $taskData 'history.sqlite3') -PathType Leaf)) {
    throw 'Diagnostic fixture database is missing'
}
$taskExecutable = (Resolve-Path -LiteralPath (Join-Path $taskRoot 'target/debug/tp-gpui.exe')).Path
$taskPreviousData = $env:TP_DIAGNOSTIC_DATA_DIR
try {
    $env:TP_DIAGNOSTIC_DATA_DIR = $taskData
    & $taskExecutable
    if ($LASTEXITCODE -ne 0) { throw "Diagnostic TP exited with code $LASTEXITCODE" }
} finally {
    $env:TP_DIAGNOSTIC_DATA_DIR = $taskPreviousData
}
