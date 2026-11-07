param(
    [Parameter(Mandatory = $true)][string]$MsiPath,
    [Parameter(Mandatory = $true)][string]$StageDirectory,
    [Parameter(Mandatory = $true)][string]$DarkPath
)

# Mutate copies of an MSI database only. Never invoke an installation action.
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$taskScripts = Join-Path $taskRoot 'scripts'
$taskSource = (Resolve-Path -LiteralPath $MsiPath).Path
$taskWork = Join-Path $taskRoot ('target/gpui-msi-policy-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($taskWork)
$taskVerify = Join-Path $taskScripts 'verify-gpui-msi.ps1'
& $taskVerify -MsiPath $taskSource -StageDirectory $StageDirectory -DarkPath $DarkPath
$taskCases = @(
    @{ Name='reinstall'; Sql="UPDATE ``Property`` SET ``Value`` = 'ous' WHERE ``Property`` = 'REINSTALLMODE'"; Message='MSI property mismatch: REINSTALLMODE' },
    @{ Name='range'; Sql="DELETE FROM ``Upgrade`` WHERE ``ActionProperty`` = 'WIX_UPGRADE_DETECTED'"; Message='MSI upgrade table does not match TP' },
    @{ Name='sequence'; Sql="UPDATE ``InstallExecuteSequence`` SET ``Sequence`` = 6601 WHERE ``Action`` = 'RemoveExistingProducts'"; Message='MSI must remove the previous product after initialization and before processing new components' },
    @{ Name='secure'; Sql="UPDATE ``Property`` SET ``Value`` = 'AUTOLAUNCHAPP' WHERE ``Property`` = 'SecureCustomProperties'"; Message='MSI upgrade detection property is not secure' }
)
foreach ($taskCase in $taskCases) {
    $taskCopy = Join-Path $taskWork ($taskCase.Name + '.msi')
    Copy-Item -LiteralPath $taskSource -Destination $taskCopy
    $taskInstaller = New-Object -ComObject WindowsInstaller.Installer
    $taskDatabase = $taskInstaller.OpenDatabase($taskCopy, 1)
    try {
        $taskView = $taskDatabase.OpenView($taskCase.Sql)
        try { [void]$taskView.Execute() }
        finally {
            [void]$taskView.Close()
            [void][Runtime.InteropServices.Marshal]::ReleaseComObject($taskView)
        }
        [void]$taskDatabase.Commit()
    } finally {
        [void][Runtime.InteropServices.Marshal]::ReleaseComObject($taskDatabase)
        [void][Runtime.InteropServices.Marshal]::ReleaseComObject($taskInstaller)
    }
    $taskRejected = $false
    try { & $taskVerify -MsiPath $taskCopy -StageDirectory $StageDirectory -DarkPath $DarkPath }
    catch {
        if ($_.Exception.Message -ne $taskCase.Message) { throw }
        $taskRejected = $true
    }
    if (-not $taskRejected) { throw "Changed MSI policy accepted: $($taskCase.Name)" }
    Write-Output "PASS: rejected changed $($taskCase.Name) policy"
}
Write-Output "MSI policy evidence: $taskWork"
