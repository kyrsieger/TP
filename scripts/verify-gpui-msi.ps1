param(
    [Parameter(Mandatory = $true)][string]$MsiPath,
    [Parameter(Mandatory = $true)][string]$StageDirectory,
    [Parameter(Mandatory = $true)][string]$DarkPath
)

# OpenDatabase mode 0 is read-only. No install sequence or custom action runs.
$ErrorActionPreference = 'Stop'
$taskMsiPath = (Resolve-Path -LiteralPath $MsiPath).Path
$taskStage = (Resolve-Path -LiteralPath $StageDirectory).Path
$taskDark = (Resolve-Path -LiteralPath $DarkPath).Path
$taskManifest = Get-Content -LiteralPath (Join-Path $taskStage 'package-manifest.json') -Raw | ConvertFrom-Json
$taskIdentity = Get-Content -LiteralPath (Join-Path (Split-Path -Parent $PSScriptRoot) 'packaging/windows/identity.json') -Raw | ConvertFrom-Json
$taskInstaller = New-Object -ComObject WindowsInstaller.Installer
$taskDatabase = $taskInstaller.OpenDatabase($taskMsiPath, 0)

function Read-TaskMsiRows([string]$Table, [string[]]$Columns) {
    $taskFields = ($Columns | ForEach-Object { '`' + $_ + '`' }) -join ','
    $taskView = $taskDatabase.OpenView('SELECT ' + $taskFields + ' FROM `' + $Table + '`')
    try {
        [void]$taskView.Execute()
        while ($taskRecord = $taskView.Fetch()) {
            try {
                $taskRow = [ordered]@{}
                for ($taskIndex = 0; $taskIndex -lt $Columns.Count; $taskIndex++) {
                    $taskRow[$Columns[$taskIndex]] = $taskRecord.StringData($taskIndex + 1)
                }
                [pscustomobject]$taskRow
            } finally { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($taskRecord) }
        }
    } finally {
        [void]$taskView.Close()
        [void][Runtime.InteropServices.Marshal]::ReleaseComObject($taskView)
    }
}

try {
    $taskProperties = @{}
    Read-TaskMsiRows 'Property' @('Property', 'Value') | ForEach-Object { $taskProperties[$_.Property] = $_.Value }
    $taskExpected = @{
        ProductName=$taskIdentity.productName; Manufacturer=$taskIdentity.publisher
        ProductVersion=$taskManifest.version; ProductLanguage='2052'; ALLUSERS='1'
        REINSTALLMODE='amus'
    }
    foreach ($taskKey in $taskExpected.Keys) {
        if ($taskProperties[$taskKey] -ne $taskExpected[$taskKey]) { throw "MSI property mismatch: $taskKey" }
    }
    if ([guid]$taskProperties.UpgradeCode -ne [guid]$taskIdentity.msiUpgradeCode) { throw 'MSI upgrade identity mismatch' }
    $taskUpgrades = @(Read-TaskMsiRows 'Upgrade' @('UpgradeCode', 'ActionProperty', 'VersionMin', 'VersionMax', 'Attributes'))
    if (-not $taskUpgrades.Count -or @($taskUpgrades | Where-Object { [guid]$_.UpgradeCode -ne [guid]$taskIdentity.msiUpgradeCode }).Count) {
        throw 'MSI upgrade table does not match TP'
    }
    $taskPolicy = Get-Content -LiteralPath (Join-Path (Split-Path -Parent $PSScriptRoot) 'packaging/windows/policy.json') -Raw | ConvertFrom-Json
    if ($taskPolicy.allowDowngrades -isnot [bool]) { throw 'allowDowngrades must be an explicit boolean' }
    $taskAllowDowngrades = $taskPolicy.allowDowngrades
    $taskUpgrade = @($taskUpgrades | Where-Object ActionProperty -eq 'WIX_UPGRADE_DETECTED')
    if ($taskUpgrade.Count -ne 1 -or $taskUpgrade[0].VersionMin -ne '0') {
        throw 'MSI upgrade range must start at the original minimum version'
    }
    if ($taskAllowDowngrades) {
        if ($taskUpgrades.Count -ne 1 -or $taskUpgrade[0].VersionMax -ne '' -or $taskUpgrade[0].Attributes -ne '257') {
            throw 'MSI must preserve the configured downgrade-compatible upgrade range'
        }
    } elseif ($taskUpgrade[0].VersionMax -ne $taskManifest.version -or $taskUpgrade[0].Attributes -ne '769') {
        throw 'MSI must preserve same-version upgrades without removing newer versions'
    }
    $taskSecure = @($taskProperties.SecureCustomProperties -split ';')
    foreach ($taskUpgradeRow in $taskUpgrades) {
        if ($taskUpgradeRow.ActionProperty -notin $taskSecure) { throw 'MSI upgrade detection property is not secure' }
    }
    $taskSequence = @(Read-TaskMsiRows 'InstallExecuteSequence' @('Action', 'Sequence'))
    $taskInitialize = @($taskSequence | Where-Object Action -eq 'InstallInitialize')
    $taskRemove = @($taskSequence | Where-Object Action -eq 'RemoveExistingProducts')
    $taskComponentsAction = @($taskSequence | Where-Object Action -eq 'ProcessComponents')
    if ($taskInitialize.Count -ne 1 -or $taskRemove.Count -ne 1 -or $taskComponentsAction.Count -ne 1 -or
        [int]$taskRemove[0].Sequence -le [int]$taskInitialize[0].Sequence -or
        [int]$taskRemove[0].Sequence -ge [int]$taskComponentsAction[0].Sequence) {
        throw 'MSI must remove the previous product after initialization and before processing new components'
    }
    $taskFiles = @(Read-TaskMsiRows 'File' @('File', 'FileName', 'FileSize'))
    if ($taskFiles.Count -ne $taskManifest.files.Count) { throw 'MSI payload count mismatch' }
    $taskSeenNames = @{}
    foreach ($taskFile in $taskFiles) {
        $taskName = ($taskFile.FileName -split '\|')[-1]
        if ($taskName -notmatch '^[a-zA-Z0-9_-][a-zA-Z0-9_.-]*$' -or $taskFile.File -notmatch '^[a-zA-Z_][a-zA-Z0-9_.]*$') {
            throw 'Unsafe MSI payload name or identifier'
        }
        if ($taskSeenNames.ContainsKey($taskName)) { throw "Duplicate MSI payload: $taskName" }
        $taskSeenNames[$taskName] = $true
        $taskEntry = @($taskManifest.files | Where-Object name -eq $taskName)
        if ($taskEntry.Count -ne 1 -or [long]$taskFile.FileSize -ne $taskEntry[0].bytes) { throw "Unexpected MSI payload: $taskName" }
        $taskStagedFile = Get-Item -LiteralPath (Join-Path $taskStage $taskName)
        if ($taskStagedFile.Length -ne $taskEntry[0].bytes -or (Get-FileHash -LiteralPath $taskStagedFile.FullName -Algorithm SHA256).Hash -ne $taskEntry[0].sha256) {
            throw "Staged payload hash mismatch: $taskName"
        }
    }
    $taskComponents = @(Read-TaskMsiRows 'Component' @('Component', 'Attributes', 'Directory_', 'KeyPath'))
    $taskMain = @($taskComponents | Where-Object Component -eq 'Path')
    if ($taskMain.Count -ne 1 -or (([int]$taskMain[0].Attributes -band 256) -eq 0) -or $taskMain[0].Directory_ -ne 'INSTALLDIR') {
        throw 'Main program is not an x64 component in INSTALLDIR'
    }
    $taskUninstall = @($taskComponents | Where-Object Component -eq 'CMP_UninstallShortcut')
    if ($taskUninstall.Count -ne 1 -or $taskUninstall[0].Directory_ -ne 'INSTALLDIR' -or
        (([int]$taskUninstall[0].Attributes -band 260) -ne 260)) {
        throw 'Uninstall shortcut must have an x64 registry key path in INSTALLDIR'
    }
    $taskRegistry = @(Read-TaskMsiRows 'Registry' @('Registry', 'Root', 'Key', 'Name', 'Component_'))
    $taskUninstallKey = @($taskRegistry | Where-Object { $_.Registry -eq $taskUninstall[0].KeyPath })
    if ($taskUninstallKey.Count -ne 1 -or $taskUninstallKey[0].Root -ne '2' -or
        $taskUninstallKey[0].Key -ne $taskIdentity.installLocationRegistryKey -or
        $taskUninstallKey[0].Name -ne 'Uninstaller Shortcut' -or
        $taskUninstallKey[0].Component_ -ne 'CMP_UninstallShortcut') {
        throw 'Uninstall shortcut key path must be its machine-wide registry marker'
    }
    $taskMedia = @(Read-TaskMsiRows 'Media' @('Cabinet'))
    if ($taskMedia.Count -ne 1 -or $taskMedia[0].Cabinet -ne '#app.cab') { throw 'Expected an embedded application cabinet' }
    $taskActions = @(Read-TaskMsiRows 'CustomAction' @('Action', 'Source', 'Target'))
    $taskAllowedActions = @('LaunchApplication', 'WixUIValidatePath', 'WixUIPrintEula', 'SetARPNOMODIFY')
    if (@($taskActions | Where-Object { $_.Action -notin $taskAllowedActions }).Count) { throw 'Unreviewed MSI custom action' }
    $taskLaunch = @($taskActions | Where-Object Action -eq 'LaunchApplication')
    if ($taskLaunch.Count -ne 1 -or $taskLaunch[0].Source -ne 'Path' -or $taskLaunch[0].Target -ne '[LAUNCHAPPARGS]') {
        throw 'MSI launch action does not reference the installed TP program'
    }
    $taskControls = @(Read-TaskMsiRows 'Control' @('Dialog_', 'Control'))
    foreach ($taskPair in @('WelcomeDlg/Next', 'InstallDirDlg/Back', 'InstallDirDlg/Next', 'ExitDialog/Finish', 'ExitDialog/OptionalCheckBox')) {
        if (-not @($taskControls | Where-Object { ($_.Dialog_ + '/' + $_.Control) -eq $taskPair }).Count) {
            throw "Missing MSI control: $taskPair"
        }
    }
    $taskEvents = @(Read-TaskMsiRows 'ControlEvent' @('Dialog_', 'Control_', 'Event', 'Argument'))
    foreach ($taskEvent in $taskEvents) {
        if (-not @($taskControls | Where-Object { $_.Dialog_ -eq $taskEvent.Dialog_ -and $_.Control -eq $taskEvent.Control_ }).Count) {
            throw "Dangling MSI control event: $($taskEvent.Dialog_)/$($taskEvent.Control_)"
        }
    }
    $taskShortcuts = @(Read-TaskMsiRows 'Shortcut' @('Shortcut', 'Target'))
    if ($taskShortcuts.Count -ne 3 -or @($taskShortcuts | Where-Object Target -eq '[!Path]').Count -ne 2) {
        throw 'Expected TP desktop, Start Menu and uninstall shortcuts'
    }
    # dark decompiles tables and extracts cabinet bytes; it never executes MSI actions.
    $taskInspection = Join-Path (Split-Path -Parent $PSScriptRoot) ('target/gpui-msi-inspection-' + [guid]::NewGuid().ToString('N'))
    [void](New-Item -ItemType Directory -Path $taskInspection)
    $taskExtracted = Join-Path $taskInspection 'extracted'
    $taskDarkLog = & $taskDark -nologo -x $taskExtracted $taskMsiPath (Join-Path $taskInspection 'decompiled.wxs') 2>&1
    $taskDarkExit = $LASTEXITCODE
    $taskDarkLog | Set-Content -LiteralPath (Join-Path $taskInspection 'dark.log')
    if ($taskDarkExit -ne 0) { throw "MSI extraction failed; see $taskInspection/dark.log" }
    foreach ($taskFile in $taskFiles) {
        $taskName = ($taskFile.FileName -split '\|')[-1]
        $taskEntry = @($taskManifest.files | Where-Object name -eq $taskName)[0]
        $taskPayload = Get-Item -LiteralPath (Join-Path $taskExtracted ('File/' + $taskFile.File))
        if ($taskPayload.Length -ne $taskEntry.bytes -or (Get-FileHash -LiteralPath $taskPayload.FullName -Algorithm SHA256).Hash -ne $taskEntry.sha256) {
            throw "MSI payload hash mismatch: $taskName"
        }
    }
    [pscustomobject]@{
        Path=$taskMsiPath; Version=$taskProperties.ProductVersion
        UpgradeCode=$taskProperties.UpgradeCode; Payloads=$taskFiles.Count
        Controls=$taskControls.Count; ControlEvents=$taskEvents.Count
        VerifiedPayloadHashes=$taskFiles.Count; InspectionDirectory=$taskInspection
        SHA256=(Get-FileHash -LiteralPath $taskMsiPath -Algorithm SHA256).Hash
    }
} finally {
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($taskDatabase)
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($taskInstaller)
}
