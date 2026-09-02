param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-p]{32}$')]
  [string]$ExtensionId,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[A-Za-z0-9._-]+$')]
  [string]$WslDistro,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^/[A-Za-z0-9._/+.-]+$')]
  [string]$LinuxHostPath
)

$ErrorActionPreference = 'Stop'
$HostName = 'io.colonist_assistant.gpu'
$DestDir = Join-Path $env:LOCALAPPDATA 'ColonistAssistant'
$Bridge = Join-Path $DestDir 'colonist-assistant-gpu-wsl-bridge.exe'
$BridgeNew = Join-Path $DestDir 'colonist-assistant-gpu-wsl-bridge.new.exe'
$Config = Join-Path $DestDir 'gpu-wsl-bridge.conf'
$Manifest = Join-Path $DestDir "$HostName.json"
$Source = Join-Path $PSScriptRoot 'gpu-wsl-bridge.cs'
$WslExe = Join-Path $env:SystemRoot 'System32\wsl.exe'
$Compiler = Join-Path ([Runtime.InteropServices.RuntimeEnvironment]::GetRuntimeDirectory()) 'csc.exe'

if (-not (Test-Path $WslExe)) {
  throw "wsl.exe was not found at $WslExe"
}
if (-not (Test-Path $Compiler)) {
  throw "Windows C# compiler was not found at $Compiler"
}
if (-not (Test-Path $Source)) {
  throw "WSL bridge source was not found at $Source"
}

& $WslExe -d $WslDistro --exec /usr/bin/test -x $LinuxHostPath
if ($LASTEXITCODE -ne 0) {
  throw "Trusted WSL GPU companion is not executable: distro=$WslDistro path=$LinuxHostPath"
}

New-Item -ItemType Directory -Force -Path $DestDir | Out-Null
Remove-Item -Force -ErrorAction SilentlyContinue $BridgeNew
$CompilerOutput = & $Compiler /nologo /optimize+ /target:exe "/out:$BridgeNew" $Source 2>&1
if ($LASTEXITCODE -ne 0) {
  throw "WSL GPU bridge compilation failed:`n$($CompilerOutput -join [Environment]::NewLine)"
}
Move-Item -Force $BridgeNew $Bridge

[System.IO.File]::WriteAllLines(
  $Config,
  @(
    "distro=$WslDistro",
    "host=$LinuxHostPath"
  ),
  [System.Text.UTF8Encoding]::new($false)
)

$ManifestObject = [ordered]@{
  name = $HostName
  description = 'Colonist Assistant WSL GPU development bridge'
  path = $Bridge
  type = 'stdio'
  allowed_origins = @("chrome-extension://$ExtensionId/")
}
$ManifestJson = $ManifestObject | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText(
  $Manifest,
  $ManifestJson,
  [System.Text.UTF8Encoding]::new($false)
)

$RegistryKey = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$HostName"
New-Item -Path $RegistryKey -Force | Out-Null
Set-Item -Path $RegistryKey -Value $Manifest

Write-Host "Installed one-time WSL development bridge for $HostName"
Write-Host "Chrome extension: $ExtensionId"
Write-Host "Trusted WSL companion: $WslDistro $LinuxHostPath"
Write-Host "Native host manifest: $Manifest"
Write-Host 'Future Linux companion rebuilds do not require rerunning this installer.'
