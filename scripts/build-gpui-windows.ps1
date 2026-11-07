param([switch]$Offline, [switch]$CheckToolsOnly)

$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskPreviousFxc = $env:GPUI_FXC_PATH
$taskCompiler = $taskPreviousFxc
if ($taskCompiler) {
    if (-not (Test-Path -LiteralPath $taskCompiler -PathType Leaf)) {
        throw 'GPUI_FXC_PATH is set but does not name an existing compiler'
    }
} else {
    $taskCommand = Get-Command fxc.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($taskCommand) {
        $taskCompiler = $taskCommand.Source
    } else {
        $taskKitsRoot = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue).KitsRoot10
        if (-not $taskKitsRoot) {
            $taskKitsRoot = Join-Path ([Environment]::GetEnvironmentVariable('ProgramFiles(x86)')) 'Windows Kits/10'
        }
        $taskCandidates = @(Get-ChildItem -LiteralPath (Join-Path $taskKitsRoot 'bin') -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } |
            Sort-Object { [version]$_.Name } -Descending |
            ForEach-Object { Join-Path $_.FullName 'x64/fxc.exe' } |
            Where-Object { Test-Path -LiteralPath $_ -PathType Leaf })
        if (-not $taskCandidates.Count) {
            throw 'Windows SDK fxc.exe was not found. Install the Windows SDK or set GPUI_FXC_PATH.'
        }
        $taskCompiler = $taskCandidates[0]
    }
}
Write-Output "GPUI shader compiler: $taskCompiler"
if ($CheckToolsOnly) { return }

Push-Location -LiteralPath $taskRoot
try {
    $env:GPUI_FXC_PATH = $taskCompiler
    $taskArguments = @('build', '-p', 'tp-gpui', '--release', '--locked')
    if ($Offline) { $taskArguments += '--offline' }
    & cargo @taskArguments
    if ($LASTEXITCODE -ne 0) { throw "GPUI release build failed (exit $LASTEXITCODE)" }
} finally {
    $env:GPUI_FXC_PATH = $taskPreviousFxc
    Pop-Location
}
