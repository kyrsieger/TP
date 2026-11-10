function Find-GpuiWindowsTools([string]$RuntimeDirectory, [string]$DumpbinPath) {
    if (-not $RuntimeDirectory -or -not $DumpbinPath) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        $visualStudio = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($LASTEXITCODE -ne 0 -or -not $visualStudio) { throw 'Visual Studio C++ tools were not found' }
        if (-not $DumpbinPath) {
            $tools = Get-ChildItem -LiteralPath (Join-Path $visualStudio 'VC/Tools/MSVC') -Directory |
                Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
            $DumpbinPath = Join-Path $tools.FullName 'bin/Hostx64/x64/dumpbin.exe'
        }
        if (-not $RuntimeDirectory) {
            $runtime = Get-ChildItem -LiteralPath (Join-Path $visualStudio 'VC/Redist/MSVC') -Directory |
                Where-Object { $_.Name -match '^\d+\.\d+\.\d+(\.\d+)?$' } |
                Sort-Object { [version]$_.Name } -Descending |
                ForEach-Object {
                    Get-ChildItem -LiteralPath (Join-Path $_.FullName 'x64') -Directory -ErrorAction SilentlyContinue |
                        Where-Object { $_.Name -match '^Microsoft\.VC14\d\.CRT$' } |
                        Sort-Object Name -Descending
                } | Select-Object -First 1
            if (-not $runtime) { throw 'Visual Studio x64 C++ redistributable files were not found' }
            $RuntimeDirectory = $runtime.FullName
        }
    }
    @{ RuntimeDirectory = $RuntimeDirectory; DumpbinPath = $DumpbinPath }
}
