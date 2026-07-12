# 从 bblanchon/pdfium-binaries 下载与 pdfium-render pdfium_latest 对齐的运行时库。
# 用法: pwsh -File scripts/download-pdfium.ps1 [-Force]

param(
    [switch]$Force
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$Triple = "x86_64-pc-windows-msvc"
$VersionFile = Join-Path $Root "assets/pdfium/VERSION"
$ChecksumFile = Join-Path $Root "assets/pdfium/pdfium-win-x64.tgz.sha256"
$DestDir = Join-Path $Root "assets/pdfium/$Triple"
$DllPath = Join-Path $DestDir "pdfium.dll"

$Version = (Get-Content $VersionFile -Raw).Trim()
$ExpectedHash = (Get-Content $ChecksumFile -Raw).Trim().ToUpperInvariant()

function Sync-ToTarget {
    param([string]$Dll)
    foreach ($profile in @("debug", "release")) {
        $out = Join-Path $Root "target/$Triple/$profile"
        New-Item -ItemType Directory -Force -Path $out | Out-Null
        Copy-Item $Dll (Join-Path $out "pdfium.dll") -Force
        Write-Host "[download-pdfium] 已同步到 target: $out/pdfium.dll"
    }
}

New-Item -ItemType Directory -Force -Path $DestDir | Out-Null

if ((Test-Path $DllPath) -and -not $Force) {
    Write-Host "[download-pdfium] 已存在: $DllPath"
    Sync-ToTarget $DllPath
    exit 0
}

$Url = "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F$Version/pdfium-win-x64.tgz"
$Archive = Join-Path $env:TEMP "pdfium-win-x64-$Version.tgz"
$ExtractDir = Join-Path $env:TEMP "pdfium-extract-$Version"

Write-Host "[download-pdfium] 下载 chromium/$Version ..."
Invoke-WebRequest -Uri $Url -OutFile $Archive -UseBasicParsing

$ActualHash = (Get-FileHash $Archive -Algorithm SHA256).Hash.ToUpperInvariant()
if ($ActualHash -ne $ExpectedHash) {
    throw "[download-pdfium] SHA256 校验失败: expected $ExpectedHash, got $ActualHash"
}
Write-Host "[download-pdfium] SHA256 校验通过"

if (Test-Path $ExtractDir) {
    Remove-Item -Recurse -Force $ExtractDir
}
New-Item -ItemType Directory -Path $ExtractDir | Out-Null
tar -xzf $Archive -C $ExtractDir

$Found = Get-ChildItem -Path $ExtractDir -Recurse -Filter "pdfium.dll" | Select-Object -First 1
if (-not $Found) {
    throw "[download-pdfium] 压缩包内未找到 pdfium.dll"
}

Copy-Item $Found.FullName $DllPath -Force
Write-Host "[download-pdfium] 已安装: $DllPath ($('{0:N0}' -f (Get-Item $DllPath).Length) bytes)"
Sync-ToTarget $DllPath
