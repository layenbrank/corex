param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('insert','force')]
    [string]$Action,

    [Parameter(Mandatory = $true)]
    [string]$Target
)

# 所有输出只用 ASCII，避免不同主机之间的编码问题

function Get-NormalizedPathItems {
    param([string]$UserPath)
    if ([string]::IsNullOrWhiteSpace($UserPath)) { return @() }
    ($UserPath -split ';' | Where-Object { $_ -and ($_.Trim()) }) | ForEach-Object { $_.TrimEnd('\\') }
}

$userPathRaw = [Environment]::GetEnvironmentVariable('PATH', 'User')
$items = Get-NormalizedPathItems -UserPath $userPathRaw
$targetNorm = $Target.TrimEnd('\\')

if ($Action -eq 'insert') {
    $exists = $false
    foreach ($p in $items) { if ($p -ieq $targetNorm) { $exists = $true; break } }
    if (-not $exists) { $items = @($items + $targetNorm) }
    $newPath = ($items -join ';')
    [Environment]::SetEnvironmentVariable('PATH', $newPath, 'User')
    Write-Host 'User PATH updated (insert)'
    Write-Host ('Path: ' + $targetNorm)
    Write-Host 'Restart the terminal to take effect'
    return
}

if ($Action -eq 'force') {
    $filtered = @()
    foreach ($p in $items) { if (-not ($p -ieq $targetNorm)) { $filtered += $p } }
    $newPath = (@($filtered + $targetNorm) -join ';')
    [Environment]::SetEnvironmentVariable('PATH', $newPath, 'User')
    Write-Host 'User PATH updated (force)'
    Write-Host ('Path: ' + $targetNorm)
    Write-Host 'Restart the terminal to take effect'
    return
}

Write-Host 'Unknown action'
exit 1
