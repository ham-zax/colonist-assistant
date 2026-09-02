param(
  [Parameter(Mandatory = $true)]
  [ValidateNotNullOrEmpty()]
  [ValidatePattern('^[a-p]{32}$')]
  [string[]]$ExtensionIds,

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
$Runtime = Join-Path $DestDir 'colonist-gpu-runtime.exe'
$RuntimeNew = Join-Path $DestDir 'colonist-gpu-runtime.new.exe'
$Config = Join-Path $DestDir 'gpu-runtime.conf'
$Manifest = Join-Path $DestDir "$HostName.json"
$Source = Join-Path $PSScriptRoot 'colonist-gpu-runtime.cs'
$WslExe = Join-Path $env:SystemRoot 'System32\wsl.exe'
$Compiler = Join-Path ([Runtime.InteropServices.RuntimeEnvironment]::GetRuntimeDirectory()) 'csc.exe'

if (-not (Test-Path $WslExe)) {
  throw "wsl.exe was not found at $WslExe"
}
if (-not (Test-Path $Compiler)) {
  throw "Windows C# compiler was not found at $Compiler"
}
if (-not (Test-Path $Source)) {
  throw "Colonist GPU Runtime source was not found at $Source"
}

& $WslExe -d $WslDistro --exec /usr/bin/test -x $LinuxHostPath
if ($LASTEXITCODE -ne 0) {
  throw "Trusted WSL GPU companion is not executable: distro=$WslDistro path=$LinuxHostPath"
}

New-Item -ItemType Directory -Force -Path $DestDir | Out-Null
Remove-Item -Force -ErrorAction SilentlyContinue $RuntimeNew
$CompilerOutput = & $Compiler /nologo /optimize+ /target:exe "/out:$RuntimeNew" $Source 2>&1
if ($LASTEXITCODE -ne 0) {
  throw "Colonist GPU Runtime compilation failed:`n$($CompilerOutput -join [Environment]::NewLine)"
}
Move-Item -Force $RuntimeNew $Runtime

[System.IO.File]::WriteAllLines(
  $Config,
  @(
    "distro=$WslDistro",
    "host=$LinuxHostPath"
  ),
  [System.Text.UTF8Encoding]::new($false)
)

$AllowedOrigins = @(
  $ExtensionIds |
    Sort-Object -Unique |
    ForEach-Object { "chrome-extension://$_/" }
)
$ManifestObject = [ordered]@{
  name = $HostName
  description = 'Colonist GPU Runtime'
  path = $Runtime
  type = 'stdio'
  allowed_origins = $AllowedOrigins
}
$ManifestJson = $ManifestObject | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText(
  $Manifest,
  $ManifestJson,
  [System.Text.UTF8Encoding]::new($false)
)

$RegistryKeys = @(
  "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$HostName",
  "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$HostName"
)
foreach ($RegistryKey in $RegistryKeys) {
  New-Item -Path $RegistryKey -Force | Out-Null
  Set-Item -Path $RegistryKey -Value $Manifest
}

Write-Host "Installed Colonist GPU Runtime for $HostName"
Write-Host "Authorized extension IDs: $($ExtensionIds -join ', ')"
Write-Host "Trusted WSL companion: $WslDistro $LinuxHostPath"
Write-Host "Native host manifest: $Manifest"
Write-Host 'Registered the same runtime for Google Chrome and Microsoft Edge.'
Write-Host 'Future Linux companion rebuilds do not require rerunning this installer.'
