param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-p]{32}$')]
  [string]$ExtensionId
)

$ErrorActionPreference = 'Stop'
$HostName = 'io.colonist_assistant.gpu'
$Root = Split-Path -Parent $PSScriptRoot
$EngineManifest = Join-Path $Root 'engine\Cargo.toml'

cargo build --manifest-path $EngineManifest --release -p colonist-catan-native-host
if ($LASTEXITCODE -ne 0) {
  throw "GPU companion build failed with exit code $LASTEXITCODE"
}

$Source = Join-Path $Root 'engine\target\release\colonist-assistant-gpu.exe'
$DestDir = Join-Path $env:LOCALAPPDATA 'ColonistAssistant'
$Binary = Join-Path $DestDir 'colonist-assistant-gpu.exe'
$Manifest = Join-Path $DestDir "$HostName.json"
New-Item -ItemType Directory -Force -Path $DestDir | Out-Null
Copy-Item -Force $Source $Binary

$Nvrtc = Get-Command 'nvrtc64*.dll' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $Nvrtc -and $env:CUDA_PATH) {
  $CudaBin = Join-Path $env:CUDA_PATH 'bin'
  $Nvrtc = Get-ChildItem -Path $CudaBin -Filter 'nvrtc64*.dll' -ErrorAction SilentlyContinue | Select-Object -First 1
}
if (-not $Nvrtc) {
  Write-Warning 'NVRTC DLL was not found on PATH/CUDA_PATH. Install the NVIDIA CUDA toolkit or make its bin directory available before starting Chrome.'
}

$ManifestObject = [ordered]@{
  name = $HostName
  description = 'Colonist Assistant CUDA strategist'
  path = $Binary
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

Write-Host "Installed $HostName for Chrome extension $ExtensionId"
Write-Host "Native host manifest: $Manifest"
