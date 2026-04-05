<#
.SYNOPSIS
    将 stormclaw 项目所需源码与配置文件复制到指定发布目录（不含构建产物与工具缓存）。

.DESCRIPTION
    适用于源码归档、离线交付或在不携带 target 的情况下同步到另一台机器再执行 cargo build。

.PARAMETER Destination
    发布目录的绝对或相对路径；若不存在则创建。

.PARAMETER Clean
    若目标目录已存在，先删除其全部内容再复制（请谨慎使用）。

.EXAMPLE
    .\scripts\publish.ps1 -Destination D:\releases\stormclaw-src

.EXAMPLE
    .\scripts\publish.ps1 -Destination ..\stormclaw-publish -Clean
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string] $Destination,

    [switch] $Clean
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$SourceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$DestPath = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Destination)

function Get-ExcludeDirectoryNames {
    $names = [System.Collections.Generic.List[string]]::new()
    foreach ($n in @(
            'target',
            '.git',
            '.claude',
            '.vs',
            '.idea',
            '.vscode',
            '.cursor'
        )) {
        $names.Add($n) | Out-Null
    }

    if (Test-Path -LiteralPath $SourceRoot -PathType Container) {
        Get-ChildItem -LiteralPath $SourceRoot -Directory -Force -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like 'target*' } |
            ForEach-Object { $names.Add($_.Name) | Out-Null }
    }

    $unique = $names.ToArray() | Sort-Object -Unique
    return , $unique
}

if ($Clean -and (Test-Path -LiteralPath $DestPath)) {
    Remove-Item -LiteralPath $DestPath -Recurse -Force
}

if (-not (Test-Path -LiteralPath $DestPath -PathType Container)) {
    New-Item -ItemType Directory -Path $DestPath -Force | Out-Null
}

$excludeDirs = Get-ExcludeDirectoryNames
$robocopyArgs = @(
    $SourceRoot,
    $DestPath,
    '/E',
    '/XD'
) + $excludeDirs + @(
    '/NFL', '/NDL', '/NJH', '/NJS', '/NC', '/NS', '/NP'
)

& robocopy @robocopyArgs
$rc = $LASTEXITCODE
# Robocopy: 0-7 视为成功（含无文件可复制等情况）
if ($rc -ge 8) {
    throw "robocopy 失败，退出码: $rc"
}

# robocopy 成功时常返回 1-3，避免让调用方误以为脚本失败
$global:LASTEXITCODE = 0

Write-Host "已发布到: $DestPath" -ForegroundColor Green
Write-Host "排除目录: $($excludeDirs -join ', ')" -ForegroundColor DarkGray
