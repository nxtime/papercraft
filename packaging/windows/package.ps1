<#
.SYNOPSIS
  Build, sign and package PaperCraft for Windows.

.DESCRIPTION
  Produces, in $env:DIST (default: dist/release):
    papercraft-<version>-windows-<arch>.msi            per-machine installer (WiX v5)
    papercraft-<version>-windows-<arch>-portable.zip   papercraft.exe + papercraft-cli.exe + papercraft-mcp.exe
    papercraft-cli-<version>-windows-<arch>.zip        papercraft-cli.exe + papercraft-mcp.exe standalone

.EXAMPLE
  pwsh packaging/windows/package.ps1 -Arch x64
  pwsh packaging/windows/package.ps1 -Arch x64 -SkipBuild
#>
param(
  [ValidateSet('x64', 'x86', 'arm64')] [string] $Arch = 'x64',
  [switch] $SkipBuild
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $Root

function Invoke-Native([string] $What, [scriptblock] $Block) {
  Write-Output "==> $What"
  & $Block
  if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

$Version = $env:PAPERCRAFT_VERSION
if (-not $Version) {
  $inPkg = $false
  foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
    if ($line -match '^\s*\[') { $inPkg = ($line.Trim() -eq '[package]'); continue }
    if ($inPkg -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $Version = $Matches[1]; break }
  }
}
if (-not $Version) { throw 'could not read [package] version from Cargo.toml' }
$MsiVersion = ($Version -split '-')[0]

$Target = switch ($Arch) { 'x64' { 'x86_64-pc-windows-msvc' } 'x86' { 'i686-pc-windows-msvc' } 'arm64' { 'aarch64-pc-windows-msvc' } }
$Dist = if ($env:DIST) { $env:DIST } else { Join-Path $Root 'dist\release' }
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

Write-Output "PaperCraft $Version for Windows $Arch ($Target)"

if (-not $SkipBuild) {
  Invoke-Native "cargo build" { cargo build --release --locked --bins }
}

$Bin = Join-Path $TargetDir 'release'
if (-not (Test-Path (Join-Path $Bin 'papercraft.exe'))) {
  $Bin = Join-Path $TargetDir "$Target\release"
}

function Get-PeHeader([string] $Path) {
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  $pe = [BitConverter]::ToInt32($bytes, 0x3C)
  return @{ Machine = [BitConverter]::ToUInt16($bytes, $pe + 4); Subsystem = [BitConverter]::ToUInt16($bytes, $pe + 0x5C) }
}
$Machine = switch ($Arch) { 'x64' { 0x8664 } 'x86' { 0x14C } 'arm64' { 0xAA64 } }
foreach ($name in @('papercraft.exe', 'papercraft-cli.exe', 'papercraft-mcp.exe')) {
  $exePath = Join-Path $Bin $name
  if (Test-Path $exePath) {
    $h = Get-PeHeader $exePath
    if ($h.Machine -ne $Machine) { throw "$name is for machine 0x$('{0:X}' -f $h.Machine), expected 0x$('{0:X}' -f $Machine) ($Arch)" }
    Write-Output "ok $($name): $Arch (0x$('{0:X}' -f $h.Machine)), PE subsystem $($h.Subsystem)"
  }
}

$Stage = Join-Path $TargetDir "windows-package\$Arch"
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item (Join-Path $Bin 'papercraft.exe'), (Join-Path $Bin 'papercraft-cli.exe'), (Join-Path $Bin 'papercraft-mcp.exe') $Stage
foreach ($f in 'LICENSE-MIT', 'LICENSE-APACHE', 'ATTRIBUTION.md', 'README.md') {
  $p = Join-Path $Root $f
  if (Test-Path $p) { Copy-Item $p $Stage }
}

& (Join-Path $PSScriptRoot 'sign.ps1') (Join-Path $Stage 'papercraft.exe') (Join-Path $Stage 'papercraft-cli.exe') (Join-Path $Stage 'papercraft-mcp.exe')

# ---- MSI ---------------------------------------------------------------------------------------
$Msi = Join-Path $Dist "papercraft-$Version-windows-$Arch.msi"
if (Get-Command wix -ErrorAction SilentlyContinue) {
  Invoke-Native 'wix build' {
    wix build (Join-Path $PSScriptRoot 'papercraft.wxs') -arch $Arch `
      -d "Version=$MsiVersion" -d "BinDir=$Stage" -d "IconPath=$(Join-Path $Root 'assets\app-icon\papercraft.ico')" `
      -o $Msi
  }
  Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($Msi, '.wixpdb'))
  & (Join-Path $PSScriptRoot 'sign.ps1') $Msi
} else {
  Write-Warning "WiX toolset not found: skipping MSI creation"
}

# ---- portable zip ------------------------------------------------------------------------------
$Portable = Join-Path $TargetDir "windows-package\papercraft-$Version-windows-$Arch-portable"
Remove-Item -Recurse -Force $Portable -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Portable | Out-Null
Copy-Item (Join-Path $Stage '*.exe') $Portable
foreach ($f in 'README.md', 'LICENSE', 'LICENSE-MIT', 'LICENSE-APACHE', 'ATTRIBUTION.md') {
  $p = Join-Path $Root $f
  if (Test-Path $p) { Copy-Item $p $Portable }
}
$Zip = Join-Path $Dist "papercraft-$Version-windows-$Arch-portable.zip"
Remove-Item -Force $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path $Portable -DestinationPath $Zip

# ---- cli zip -----------------------------------------------------------------------------------
$CliPortable = Join-Path $TargetDir "windows-package\papercraft-cli-$Version-windows-$Arch"
Remove-Item -Recurse -Force $CliPortable -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $CliPortable | Out-Null
Copy-Item (Join-Path $Stage 'papercraft-cli.exe'), (Join-Path $Stage 'papercraft-mcp.exe') $CliPortable
foreach ($f in 'README.md', 'LICENSE', 'LICENSE-MIT', 'LICENSE-APACHE', 'ATTRIBUTION.md') {
  $p = Join-Path $Root $f
  if (Test-Path $p) { Copy-Item $p $CliPortable }
}
$CliZip = Join-Path $Dist "papercraft-cli-$Version-windows-$Arch.zip"
Remove-Item -Force $CliZip -ErrorAction SilentlyContinue
Compress-Archive -Path $CliPortable -DestinationPath $CliZip

# Smoke test
Invoke-Native 'papercraft-cli --help' { & (Join-Path $Stage 'papercraft-cli.exe') --help | Out-Null }

$artifacts = @($Zip, $CliZip)
if (Test-Path $Msi) { $artifacts += $Msi }
Get-Item $artifacts | Format-Table Name, Length
