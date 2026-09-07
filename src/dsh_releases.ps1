$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$versions = @()
$page = 1
do {
    $response = Invoke-RestMethod -UseBasicParsing -TimeoutSec 10 -Headers @{
        'User-Agent' = 'DSH-Launcher'
        'Accept' = 'application/vnd.github+json'
    } -Uri "https://api.github.com/repos/deepseek-ai/deepseek-harness/releases?per_page=100&page=$page"
    $versions += @($response | Select-Object tag_name, draft)
    $page++
} while (@($response).Count -eq 100)
[Console]::Out.Write((ConvertTo-Json -InputObject @($versions) -Compress))
