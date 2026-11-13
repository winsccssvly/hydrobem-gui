<#
.SYNOPSIS
    Checks that a Python environment holds what this repository says it should: the GUI wheel
    of the current engine and build.

.DESCRIPTION
    Read only. Compares, in the environment of -Python:
    - hydrobem_gui's bin\hydrobem.dll with the SHA-256 of engine/IMPORT.json (the imported DLL),
    - hydrobem_gui's bin\HydroBEM.exe with target\release\HydroBEM.exe (the last release build),
    Prints one line per check and exits 1 when any is stale: then rebuild and reinstall the wheel
    (docs/DEVELOPMENT.md).

.EXAMPLE
    ./scripts/check-env.ps1 -Python C:\Users\me\miniconda3\envs\hydrobem-gui\python.exe
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Python
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$stale = 0
function Report([bool] $ok, [string] $what, [string] $detail) {
    $mark = if ($ok) { 'ok   ' } else { 'STALE' }
    Write-Host "$mark $what$(if ($detail) { ": $detail" })"
    if (-not $ok) { $script:stale++ }
}

$site = (& $Python -c "import sysconfig; print(sysconfig.get_path('purelib'))").Trim()

# the GUI wheel: its DLL and its program
$bin = Join-Path $site 'hydrobem_gui\bin'
$import = Get-Content (Join-Path $root 'engine\IMPORT.json') -Raw | ConvertFrom-Json
$dll = Join-Path $bin 'hydrobem.dll'
if (Test-Path $dll) {
    $sha = (Get-FileHash $dll -Algorithm SHA256).Hash.ToLower()
    Report ($sha -eq $import.sha256.'hydrobem.dll') 'hydrobem_gui DLL' "HydroBEM $($import.source_commit.Substring(0, 12)) imported"
} else {
    Report $false 'hydrobem_gui DLL' "no $dll (the wheel is not installed)"
}
$exe = Join-Path $bin 'HydroBEM.exe'
$release = Join-Path $root 'target\release\HydroBEM.exe'
if ((Test-Path $exe) -and (Test-Path $release)) {
    $same = (Get-FileHash $exe).Hash -eq (Get-FileHash $release).Hash
    Report $same 'hydrobem_gui HydroBEM.exe' $(if ($same) { 'the last release build' } else { 'not the last release build' })
} else {
    Report $false 'hydrobem_gui HydroBEM.exe' 'missing (the wheel or the release build)'
}

if ($stale) { exit 1 }
