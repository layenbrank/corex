<#
.SYNOPSIS
    在 Windows 上安装或重新安装 Corex CLI。

.DESCRIPTION
    从 GitHub Releases 下载发布资产，对照该 Release 的 SHA256SUMS.txt 校验
    SHA-256，解包，并报告落盘结果。面向两类场景：

      * 首次安装；
      * 无法自更新的安装（只读目录、由包管理器接管、或内网隔离的机器）。

    已经装有 `corex` 的机器应优先用 `corex update`，它在原地完成同样的工作。

.PARAMETER Version
    要安装的确切版本，如 6.0.1。缺省为最新的稳定版。
    用于解析 `v<version>` 标签。

.PARAMETER Channel
    stable（默认）、alpha、beta 或 rc。指定 -Version 时忽略。

.PARAMETER InstallDir
    目标目录。缺省为 %LOCALAPPDATA%\corex\bin，该目录无需提权即可写，
    以便日后自更新仍能工作。

.PARAMETER AddToPath
    当 InstallDir 不在*用户* PATH 中时追加进去。

.PARAMETER Force
    覆盖已存在且报告*不同*版本的安装。
    重装磁盘上已有版本不需要该开关。

.PARAMETER ZipUrl
    从本地文件或备用 URL 安装，而不走 GitHub Releases。
    此时校验仅限于你传入的 -ExpectedSha256。

.PARAMETER ExpectedSha256
    下载包应有的 SHA-256，用于 -ZipUrl 安装。

.EXAMPLE
    irm https://github.com/layenbrank/corex/releases/latest/download/install.ps1 | iex

.EXAMPLE
    .\install.ps1 -Version 6.0.0 -InstallDir C:\tools\corex -AddToPath
#>
[CmdletBinding()]
param(
    [string]$Version,
    [ValidateSet('stable', 'alpha', 'beta', 'rc')]
    [string]$Channel = 'stable',
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'corex\bin'),
    [switch]$AddToPath,
    [switch]$Force,
    [string]$ZipUrl,
    [string]$ExpectedSha256
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# 诊断信息走 host，不走成功流，这样 `irm | iex` 的用户能看到
# 进度，而捕获管道的调用方仍然只拿到摘要。
function Write-Step { param([string]$Message) Write-Host "==> $Message" -ForegroundColor Cyan }
function Write-Warn { param([string]$Message) Write-Warning $Message }

# `corex 6.0.1` -> `6.0.1`，与 `corex update` 换件前使用的规则一致。
function Read-CorexVersion {
    param([string]$Path)
    ((& $Path --version 2>&1 | Out-String).Trim() -split '\s+')[-1]
}

$repository = 'layenbrank/corex'
$timeoutSec = 300

# --- 前置检查 ---------------------------------------------------------------

$isWindowsHost = $env:OS -eq 'Windows_NT'
if (-not $isWindowsHost) {
    throw "This script installs the Windows build. Use the archive for your platform instead."
}

$architecture = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
$slug = "windows-$architecture"
if ($slug -ne 'windows-x64') {
    throw "No published build for '$slug'. Only windows-x64 artefacts are released today."
}

# --- 解析 Release ----------------------------------------------------------

$tls12 = [Net.SecurityProtocolType]::Tls12
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor $tls12

if ($Version) {
    $tag = if ($Version.StartsWith('v')) { $Version } else { "v$Version" }
    $releaseApi = "https://api.github.com/repos/$repository/releases/tags/$tag"
} else {
    $releaseApi = "https://api.github.com/repos/$repository/releases"
}

Write-Step "解析 Release（$Channel）"
$headers = @{ 'User-Agent' = 'corex-install'; 'Accept' = 'application/vnd.github+json' }
if ($env:COREX_GITHUB_TOKEN) { $headers['Authorization'] = "Bearer $($env:COREX_GITHUB_TOKEN)" }
elseif ($env:GH_TOKEN) { $headers['Authorization'] = "Bearer $($env:GH_TOKEN)" }
elseif ($env:GITHUB_TOKEN) { $headers['Authorization'] = "Bearer $($env:GITHUB_TOKEN)" }

try {
    $payload = Invoke-RestMethod -Uri $releaseApi -Headers $headers -TimeoutSec 60
} catch {
    throw "无法查询 GitHub Releases：$($_.Exception.Message)`n若本机通过代理上网，请先设置 `$env:HTTPS_PROXY。"
}

$release = if ($Version) {
    $payload
} else {
    $wanted = switch ($Channel) {
        'stable' { $null }
        'alpha' { '-alpha' }
        'beta' { '-beta' }
        'rc' { '-rc' }
    }
    $candidates = $payload | Where-Object {
        -not $_.draft -and
        ($(if ($null -eq $wanted) { -not $_.prerelease } else { $_.prerelease -and $_.tag_name -match [regex]::Escape($wanted) }))
    }
    if (-not $candidates) { throw "通道 '$Channel' 上没有可用 Release。" }
    $candidates | Sort-Object { [version]($_.tag_name.TrimStart('v') -replace '-.*$', '') } -Descending | Select-Object -First 1
}

$tag = $release.tag_name
Write-Step "目标版本 $tag"

# --- 下载 ------------------------------------------------------------------

$archiveName = "corex-$tag-$slug.zip"

if ($ZipUrl) {
    $archiveName = Split-Path $ZipUrl -Leaf
} else {
    $asset = $release.assets | Where-Object { $_.name -eq $archiveName } | Select-Object -First 1
    if (-not $asset) {
        $names = ($release.assets | ForEach-Object { $_.name }) -join ', '
        throw "Release $tag 中没有 $archiveName。可用资产：$names"
    }
}

$work = Join-Path ([IO.Path]::GetTempPath()) ("corex-install-" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Force -Path $work | Out-Null

try {
    # --- 期望摘要，在传输之前先把它们收集齐 ---
    $expected = @()
    if ($ExpectedSha256) { $expected += $ExpectedSha256.ToLowerInvariant() }
    if (-not $ZipUrl) {
        if ($asset.digest) {
            $expected += ($asset.digest -replace '^sha256:', '').ToLowerInvariant()
        }
        $sumsAsset = $release.assets | Where-Object { $_.name -eq 'SHA256SUMS.txt' } | Select-Object -First 1
        if ($sumsAsset) {
            $sumsText = Invoke-RestMethod -Uri $sumsAsset.browser_download_url -Headers $headers -TimeoutSec 60
            $entry = ($sumsText -split "`n") | Where-Object { $_ -match [regex]::Escape($archiveName) } | Select-Object -First 1
            if ($entry -and $entry -match '([0-9a-fA-F]{64})') {
                $expected += $Matches[1].ToLowerInvariant()
            }
        }
    }
    if (-not $expected) {
        throw "无法获得 $archiveName 的 SHA-256 校验和，已中止安装。"
    }

    $archivePath = Join-Path $work $archiveName
    if ($ZipUrl -and (Test-Path $ZipUrl)) {
        Write-Step "使用本地文件 $ZipUrl"
        Copy-Item $ZipUrl $archivePath
    } else {
        $uri = if ($ZipUrl) { $ZipUrl } else { $asset.browser_download_url }
        Write-Step "下载 $archiveName"
        $sw = [Diagnostics.Stopwatch]::StartNew()
        Invoke-WebRequest -Uri $uri -OutFile $archivePath -Headers @{ 'User-Agent' = 'corex-install' } -TimeoutSec $timeoutSec
        $sw.Stop()
        Write-Step ("下载完成 {0:N1} MB / {1:N1}s" -f ((Get-Item $archivePath).Length / 1MB), $sw.Elapsed.TotalSeconds)
    }

    $actual = (Get-FileHash $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    foreach ($want in $expected) {
        if ($want -ne $actual) {
            throw "SHA-256 校验失败：期望 $want，实际 $actual。已中止，未安装任何文件。"
        }
    }
    Write-Step "SHA-256 校验通过（$actual）"

    # --- 解包 ---
    $staging = Join-Path $work 'pkg'
    Expand-Archive -Path $archivePath -DestinationPath $staging -Force

    $cli = Join-Path $staging 'corex.exe'
    if (-not (Test-Path $cli)) {
        throw "压缩包中缺少 corex.exe。"
    }

    # --- 安装 ---
    $target = Join-Path $InstallDir 'corex.exe'
    if (Test-Path $target) {
        $current = Read-CorexVersion $target
        if (-not $Force -and $current -ne $tag.TrimStart('v')) {
            throw "目标目录已有 corex（$current），与 $tag 不同。确认覆盖请加 -Force。"
        }
        Write-Step "覆盖已有安装（$current）"
    }
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

    # 停掉运行中的守护进程，免得复制期间它的可执行文件被锁定。
    $daemonName = 'corex-daemon.exe'
    $daemonRunning = @(Get-Process -Name 'corex-daemon' -ErrorAction SilentlyContinue)
    if ($daemonRunning.Count -gt 0) {
        Write-Step "停止正在运行的 corex-daemon"
        foreach ($proc in $daemonRunning) {
            try { $proc.CloseMainWindow() | Out-Null } catch { }
            try { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue } catch { }
        }
        Start-Sleep -Milliseconds 400
    }

    Get-ChildItem $staging -File | ForEach-Object {
        Copy-Item $_.FullName (Join-Path $InstallDir $_.Name) -Force
    }
    Write-Step "已安装到 $InstallDir"

    $installed = Read-CorexVersion (Join-Path $InstallDir 'corex.exe')
    Write-Step "验证：corex $installed"

    if ($installed -ne $tag.TrimStart('v')) {
        Write-Warn "corex --version 报告 '$installed'，与 Release $tag 不一致，请检查。"
    }

    # --- PATH ---
    $userPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    $onPath = ($userPath -split ';') | Where-Object { $_ -and $_.TrimEnd('\') -ieq $InstallDir.TrimEnd('\') }
    if ($AddToPath -and -not $onPath) {
        $newPath = if ([string]::IsNullOrWhiteSpace($userPath)) { $InstallDir } else { "$userPath;$InstallDir" }
        [Environment]::SetEnvironmentVariable('PATH', $newPath, 'User')
        Write-Step "已将 $InstallDir 追加到用户 PATH（新终端生效）"
    } elseif (-not $onPath) {
        Write-Warn "$InstallDir 不在 PATH 中。重新运行并加 -AddToPath，或手动添加。"
    }

    Write-Host ''
    Write-Host "Corex $tag 安装完成。" -ForegroundColor Green
    Write-Host "  CLI：$(Join-Path $InstallDir 'corex.exe')"
    Write-Host ''
    Write-Host '后续升级：corex update'
    Write-Host "  该目录位于用户空间，无需管理员权限即可自更新。"
} finally {
    if (Test-Path $work) { Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue }
}
