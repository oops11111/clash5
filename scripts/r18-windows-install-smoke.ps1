param(
  [Parameter(Mandatory = $true)][string]$Clash5OldInstaller,
  [Parameter(Mandatory = $true)][string]$Clash5NewInstaller
)

$ErrorActionPreference = 'Stop'
$upstreamOld = Join-Path $env:RUNNER_TEMP 'Clash.Verge_2.5.7_x64-setup.exe'
$upstreamNew = Join-Path $env:RUNNER_TEMP 'Clash.Verge_2.5.8_x64-setup.exe'
$clash5InstallDir = Join-Path $env:RUNNER_TEMP 'clash5-isolation-under-test'
$upstreamConfigDir = Join-Path $env:APPDATA 'io.github.clash-verge-rev.clash-verge-rev'
$sentinelPath = Join-Path $upstreamConfigDir 'clash5-r18-config-sentinel.txt'

function Invoke-Setup([string]$Path, [string[]]$Arguments) {
  $process = Start-Process -FilePath $Path -ArgumentList $Arguments -PassThru
  if (-not $process.WaitForExit(600000)) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    throw "Installer timed out: $Path"
  }
  if ($process.ExitCode -ne 0) { throw "Installer failed with exit code $($process.ExitCode): $Path" }
}

function Get-InstallRecord([string]$DisplayName) {
  $roots = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall'
  )
  foreach ($root in $roots) {
    if (-not (Test-Path -LiteralPath $root)) { continue }
    foreach ($key in Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue) {
      $record = Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction SilentlyContinue
      if ($record.DisplayName -eq $DisplayName) { return $record }
    }
  }
  return $null
}

function Get-InstallDirectory($Record) {
  if (-not $Record -or -not $Record.UninstallString) { throw 'Application uninstall registration is missing' }
  $match = [regex]::Match([string]$Record.UninstallString, '^\s*"([^"]+)"')
  if ($match.Success) { return Split-Path -Parent $match.Groups[1].Value }
  return Split-Path -Parent ([string]$Record.UninstallString).Trim()
}

function Get-TreeChecksum([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Container)) { return 'MISSING' }
  $entries = [System.Collections.Generic.List[string]]::new()
  foreach ($item in Get-ChildItem -LiteralPath $Path -Force -Recurse | Sort-Object FullName) {
    $relative = $item.FullName.Substring($Path.Length).TrimStart('\').Replace('\', '/')
    if ($item.PSIsContainer) { $entries.Add("D|$relative") }
    else {
      $digest = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
      $entries.Add("F|$relative|$digest")
    }
  }
  $bytes = [Text.Encoding]::UTF8.GetBytes(($entries -join [Environment]::NewLine))
  $sha = [Security.Cryptography.SHA256]::Create()
  try { return [Convert]::ToHexString($sha.ComputeHash($bytes)).ToLowerInvariant() }
  finally { $sha.Dispose() }
}

function Assert-Installed([string]$DisplayName, [string]$ExpectedVersion) {
  $record = Get-InstallRecord $DisplayName
  if (-not $record) { throw "$DisplayName is not registered as installed" }
  if ($ExpectedVersion -and $record.DisplayVersion -ne $ExpectedVersion) {
    throw "$DisplayName version is '$($record.DisplayVersion)', expected '$ExpectedVersion'"
  }
  $directory = Get-InstallDirectory $record
  if (-not (Test-Path -LiteralPath $directory -PathType Container)) { throw "$DisplayName install directory is missing: $directory" }
  return $directory
}

function Assert-Running([string]$Executable) {
  $process = Start-Process -FilePath $Executable -PassThru
  Start-Sleep -Seconds 8
  $process.Refresh()
  if ($process.HasExited) { throw "$Executable exited during launch with code $($process.ExitCode)" }
  Stop-Process -Id $process.Id -Force
  Wait-Process -Id $process.Id -Timeout 15 -ErrorAction SilentlyContinue
}

function Uninstall-IfPresent([string]$DisplayName) {
  $record = Get-InstallRecord $DisplayName
  if (-not $record) { return }
  $uninstaller = Join-Path (Get-InstallDirectory $record) 'uninstall.exe'
  if (Test-Path -LiteralPath $uninstaller) { Invoke-Setup $uninstaller @('/S') }
}

try {
  Invoke-WebRequest -Uri 'https://github.com/clash-verge-rev/clash-verge-rev/releases/download/v2.5.7/Clash.Verge_2.5.7_x64-setup.exe' -OutFile $upstreamOld
  Invoke-WebRequest -Uri 'https://github.com/clash-verge-rev/clash-verge-rev/releases/download/v2.5.8/Clash.Verge_2.5.8_x64-setup.exe' -OutFile $upstreamNew
  $expected = @{
    $upstreamOld = '91990f1c1d7bfd3ca3de0faafef462b2305dc24afcf5c0dfcbe8d38dce0bf8b8'
    $upstreamNew = '821c0f5957f84c8f7d14532044840e8280d708af24a6a143db360b49bd1e7302'
  }
  foreach ($path in @($upstreamOld, $upstreamNew)) {
    $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected[$path]) { throw "Official upstream installer checksum mismatch: $path" }
  }

  Invoke-Setup $upstreamOld @('/S')
  $upstreamDir = Assert-Installed 'Clash Verge' '2.5.7'
  New-Item -ItemType Directory -Path $upstreamConfigDir -Force | Out-Null
  Set-Content -LiteralPath $sentinelPath -Value 'clash5 installer isolation fixture' -Encoding utf8
  $beforeClash5Install = Get-TreeChecksum $upstreamConfigDir

  Invoke-Setup $Clash5OldInstaller @('/S', "/D=$clash5InstallDir")
  $clash5Dir = Assert-Installed 'clash5' '2.5.9'
  if ($clash5Dir -ne $clash5InstallDir) { throw "clash5 ignored its isolated install directory: $clash5Dir" }
  if ((Get-TreeChecksum $upstreamConfigDir) -ne $beforeClash5Install) {
    throw 'Installing clash5 changed the Clash Verge Rev configuration checksum'
  }

  Invoke-Setup $upstreamNew @('/S', '/UPDATE')
  $upstreamDir = Assert-Installed 'Clash Verge' '2.5.8'
  $clash5Dir = Assert-Installed 'clash5' '2.5.9'
  $beforeClash5Update = Get-TreeChecksum $upstreamConfigDir

  Invoke-Setup $Clash5NewInstaller @('/S', '/UPDATE')
  $clash5Dir = Assert-Installed 'clash5' '2.5.10'
  $upstreamDir = Assert-Installed 'Clash Verge' '2.5.8'
  if ((Get-TreeChecksum $upstreamConfigDir) -ne $beforeClash5Update) {
    throw 'Updating clash5 changed the Clash Verge Rev configuration checksum'
  }

  Assert-Running (Join-Path $upstreamDir 'clash-verge.exe')
  Assert-Running (Join-Path $clash5Dir 'clash5.exe')
  $checksumBeforeClash5Uninstall = Get-TreeChecksum $upstreamConfigDir
  Uninstall-IfPresent 'clash5'
  if (Get-InstallRecord 'clash5') { throw 'clash5 remained registered after uninstall' }
  if (-not (Test-Path -LiteralPath (Join-Path $upstreamDir 'clash-verge.exe'))) {
    throw 'Uninstalling clash5 removed the Clash Verge Rev executable'
  }
  if ((Get-TreeChecksum $upstreamConfigDir) -ne $checksumBeforeClash5Uninstall) {
    throw 'Uninstalling clash5 changed the Clash Verge Rev configuration checksum'
  }

  Uninstall-IfPresent 'Clash Verge'
  if (Get-InstallRecord 'Clash Verge') { throw 'Clash Verge Rev remained registered after uninstall' }
  if (Get-InstallRecord 'clash5') { throw 'Uninstalling Clash Verge Rev changed clash5 registration' }
  Write-Output "Upstream configuration SHA-256 before/after clash5 install/update/uninstall: $beforeClash5Install"
} finally {
  Get-Process -Name 'clash-verge', 'clash5' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  try { Uninstall-IfPresent 'clash5' } catch { Write-Warning $_ }
  try { Uninstall-IfPresent 'Clash Verge' } catch { Write-Warning $_ }
}
