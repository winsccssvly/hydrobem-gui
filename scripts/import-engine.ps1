<#
.SYNOPSIS
    Imports the HydroBEM engine DLL (hydrobem.dll, its C ABI) from a HydroBEM checkout into engine/.

.DESCRIPTION
    Copies the DLL HydroBEM built (target\release\hydrobem.dll, `cargo build --release -p
    hydrobem-c` in HydroBEM), its header crates\hydrobem-c\include\hydrobem.h and the licence files
    into engine/, and writes engine/IMPORT.json with the source commit, whether the engine sources
    were dirty, the ABI version of the header, the times of the DLL and of the source commit and
    the SHA-256 of every imported file. The HydroBEM checkout is only read, never modified or
    built: HydroBEM makes the DLL. The DLL itself is not committed (.gitignore); the header and the
    record are.

.EXAMPLE
    ./scripts/import-engine.ps1 -Source ..\HydroBEM
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Source
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dll = Join-Path $Source 'target\release\hydrobem.dll'
$header = Join-Path $Source 'crates\hydrobem-c\include\hydrobem.h'
$target = Join-Path $root 'engine'

foreach ($path in @($dll, $header, "$Source\LICENSE")) {
    if (-not (Test-Path $path)) { throw "Missing HydroBEM file: $path (build the DLL in HydroBEM first)" }
}

if (Test-Path $target) { Remove-Item -Recurse -Force $target }
New-Item -ItemType Directory -Force $target | Out-Null

Copy-Item $dll, $header $target
# Licence files that must ship with the engine when it is redistributed.
$legal = Join-Path $target 'legal'
New-Item -ItemType Directory -Force $legal | Out-Null
foreach ($name in @('LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md')) {
    Copy-Item (Join-Path $Source $name) $legal
}
if (Test-Path "$Source\licenses") {
    Copy-Item -Recurse "$Source\licenses" (Join-Path $legal 'licenses')
}

$text = Get-Content $header -Raw
if ($text -notmatch '#define HB_ABI_VERSION (\d+)') { throw 'HB_ABI_VERSION not found in hydrobem.h.' }
$abiVersion = [int]$Matches[1]

$commit = (git -C $Source rev-parse HEAD).Trim()
$describe = (git -C $Source describe --always --dirty --abbrev=12).Trim()
$commitTime = [DateTimeOffset]::FromUnixTimeSeconds([long](git -C $Source log -1 --format=%ct)).UtcDateTime
# Dirty means uncommitted changes in what the DLL is built from.
$dirty = [bool](git -C $Source status --porcelain --untracked-files=no -- crates/hydrobem crates/hydrobem-c Cargo.toml Cargo.lock .cargo)
$built = (Get-Item $dll).LastWriteTimeUtc
if ($built -lt $commitTime) {
    Write-Warning "hydrobem.dll ($($built.ToString('u'))) is older than the HydroBEM commit $describe ($($commitTime.ToString('u'))): it may be built from an earlier commit."
}

$checksums = [ordered]@{}
Get-ChildItem $target -Recurse -File | Sort-Object FullName | ForEach-Object {
    $relative = $_.FullName.Substring($target.Length + 1).Replace('\', '/')
    $checksums[$relative] = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
}

$manifest = [ordered]@{
    schema_version   = 2
    library          = 'hydrobem.dll'
    abi_version      = $abiVersion
    source_commit    = $commit
    source_describe  = $describe
    source_dirty     = $dirty
    source_commit_utc = $commitTime.ToString('o')
    dll_built_utc    = $built.ToString('o')
    imported_utc     = (Get-Date).ToUniversalTime().ToString('o')
    sha256           = $checksums
}
$json = $manifest | ConvertTo-Json -Depth 4
# UTF-8 without a byte-order mark, so that include_str! and serde read it as is.
[System.IO.File]::WriteAllText((Join-Path $target 'IMPORT.json'), $json + "`n", (New-Object System.Text.UTF8Encoding $false))

Write-Host "Imported hydrobem.dll (ABI $abiVersion) of HydroBEM $describe into engine/"
