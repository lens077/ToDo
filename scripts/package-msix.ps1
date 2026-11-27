<#
  Virgo MSIX 打包/签名脚本（在 Windows 测试机上运行）
  ------------------------------------------------------------------
  前置：Windows 10/11 SDK（提供 makeappx.exe 与 signtool.exe），或
        `winget install Microsoft.WindowsSDK`；
        PowerShell 5.1+（自带）。
  用法（在仓库根目录）：
    pwsh -File scripts\package-msix.ps1 `
      -PayloadDir apps\desktop\src-tauri\target\x86_64-pc-windows-msvc\release `
      -Version 0.1.1.0 -Publisher "CN=Virgo Test" -Sign
  说明：-Sign 会生成/复用自签名证书（主题必须等于 Publisher），
        把证书加入「受信任人」存储，并用 signtool 对 MSIX 签名。
        企业签名请改用 -CertificatePath 指定 pfx。
#>
param(
  [Parameter(Mandatory = $true)][string]$PayloadDir,
  [string]$OutputDir = "dist\msix",
  [string]$Version = "0.1.1.0",
  [string]$Publisher = "CN=Virgo Test",
  [string]$ExeName = "Sumery Virgo.exe",
  [string]$CertificatePath,
  [string]$CertificatePassword = "virgo",
  [switch]$Sign
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$manifestTemplate = Join-Path $repoRoot "apps\desktop\src-tauri\msix\AppxManifest.xml"
$assetsDir = Join-Path $repoRoot "apps\desktop\src-tauri\msix\Assets"
$payload = (Resolve-Path $PayloadDir).Path
$out = Join-Path $repoRoot $OutputDir
New-Item -ItemType Directory -Force -Path $out | Out-Null
$layout = Join-Path $out "layout"
if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
New-Item -ItemType Directory -Force -Path $layout | Out-Null

function Find-SdkTool([string]$name) {
  $roots = @(
    "C:\Program Files (x86)\Windows Kits\10\bin",
    "C:\Program Files\Windows Kits\10\bin"
  )
  foreach ($root in $roots) {
    if (-not (Test-Path $root)) { continue }
    $hit = Get-ChildItem -Path $root -Recurse -Filter $name -ErrorAction SilentlyContinue |
      Where-Object { $_.FullName -match "x64" } |
      Select-Object -First 1
    if ($hit) { return $hit.FullName }
  }
  throw "找不到 $name，请安装 Windows 10/11 SDK：winget install Microsoft.WindowsSDK"
}

$makeappx = Find-SdkTool "makeappx.exe"
$signtool = $null
if ($Sign) { $signtool = Find-SdkTool "signtool.exe" }

if (-not (Test-Path (Join-Path $payload $ExeName))) {
  throw "payload 里没有 $ExeName，检查 -PayloadDir 是否指向 tauri 的 release 目录"
}

Copy-Item -Path (Join-Path $payload "*") -Destination $layout -Recurse -Force
Copy-Item -Path $assetsDir -Destination (Join-Path $layout "Assets") -Recurse -Force
$manifest = (Get-Content $manifestTemplate -Raw).
  Replace("__PUBLISHER__", $Publisher).
  Replace("__VERSION__", $Version).
  Replace("__EXE__", $ExeName)
$utf8 = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText((Join-Path $layout "AppxManifest.xml"), $manifest, $utf8)

$msix = Join-Path $out "virgo-$Version-x64.msix"
& $makeappx pack /d $layout /p $msix /o
if ($LASTEXITCODE -ne 0) { throw "makeappx failed: $LASTEXITCODE" }

if ($Sign) {
  if ($CertificatePath) {
    $pfx = (Resolve-Path $CertificatePath).Path
    & $signtool sign /fd SHA256 /f $pfx /p $CertificatePassword $msix
  } else {
    $cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $Publisher } | Select-Object -First 1
    if (-not $cert) {
      $cert = New-SelfSignedCertificate -Type Custom -Subject $Publisher `
        -KeyUsage DigitalSignature -FriendlyName "Virgo MSIX Test" `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
    }
    # 签名需要私钥导出到临时 pfx
    $tmpPfx = Join-Path $out "virgo-test.pfx"
    $secure = ConvertTo-SecureString -String $CertificatePassword -Force -AsPlainText
    Export-PfxCertificate -Cert $cert -FilePath $tmpPfx -Password $secure | Out-Null
    & $signtool sign /fd SHA256 /f $tmpPfx /p $CertificatePassword $msix
    # 把证书放进「受信任人」以便安装（需要管理员）
    $cer = Join-Path $out "virgo-test.cer"
    Export-Certificate -Cert $cert -FilePath $cer | Out-Null
    Write-Host "请以管理员执行：Import-Certificate -FilePath $cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople"
    Remove-Item $tmpPfx -Force
  }
}

Write-Host "MSIX: $msix"
Write-Host "安装：Add-AppxPackage -Path `"$msix`"（或双击）"
