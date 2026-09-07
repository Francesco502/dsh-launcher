param([string]$Version, [string]$Stage)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$base = "https://github.com/Francesco502/dsh-launcher/releases/download/v$Version"
$zipName = 'DSH-Launcher-Portable-x64.zip'
foreach ($name in @($zipName, "$zipName.sha256", 'release-manifest.json')) {
    Invoke-WebRequest -UseBasicParsing -TimeoutSec 120 -Uri "$base/$name" -OutFile (Join-Path $Stage $name)
}
$manifest = Get-Content -Raw -LiteralPath (Join-Path $Stage 'release-manifest.json') | ConvertFrom-Json
if ($manifest.schema_version -ne 1 -or $manifest.self_update_protocol -ne 1 -or
    $manifest.project -cne 'Francesco502/dsh-launcher' -or $manifest.version -cne $Version -or
    $manifest.tag -cne "v$Version" -or $manifest.target -cne 'x86_64-pc-windows-gnu' -or
    $manifest.architecture -cne 'x86_64' -or $manifest.commit -notmatch '^[a-f0-9]{40}$') {
    throw 'Release manifest identity or update protocol mismatch.'
}
function Check-Hash([string]$Name, [string]$File) {
    $assets = @($manifest.assets | Where-Object { $_.name -ceq $Name })
    if ($assets.Count -ne 1 -or $assets[0].sha256 -notmatch '^[a-f0-9]{64}$' -or
        (Get-FileHash -Algorithm SHA256 -LiteralPath $File).Hash -ine $assets[0].sha256) {
        throw "SHA-256 mismatch: $Name"
    }
}
$zipPath = Join-Path $Stage $zipName
Check-Hash $zipName $zipPath
Check-Hash "$zipName.sha256" (Join-Path $Stage "$zipName.sha256")
$checksum = (Get-Content -Raw -LiteralPath (Join-Path $Stage "$zipName.sha256")).Trim()
if ($checksum -cnotmatch '^([a-f0-9]{64})  DSH-Launcher-Portable-x64\.zip$' -or
    (Get-FileHash -Algorithm SHA256 -LiteralPath $zipPath).Hash -ine $Matches[1]) { throw 'ZIP checksum mismatch.' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead($zipPath)
try {
    $prefix = 'DSH-Launcher-Portable-x64/'
    $files = @('DSH-Launcher.exe', 'portable.flag', 'runtime-manifest.json', 'dshctl.cmd')
    $seen = @{}
    foreach ($entry in $archive.Entries) {
        $name = $entry.FullName.Replace('\', '/')
        if ($seen.ContainsKey($name)) { throw 'Duplicate ZIP entry.' }
        $seen[$name] = $true
        if ($name -ceq $prefix -and $entry.Length -eq 0) { continue }
        if ($name -cnotin @($files | ForEach-Object { $prefix + $_ }) -or $entry.Length -gt 16MB -or
            (($entry.ExternalAttributes -shr 16) -band 0xF000) -eq 0xA000) { throw "Invalid ZIP entry: $name" }
    }
    if ($seen.Count -ne 5) { throw 'Incomplete portable ZIP.' }
    foreach ($file in $files) { if (!$seen.ContainsKey($prefix + $file)) { throw "Missing $file" } }
    $candidate = Join-Path $Stage 'candidate'
    [IO.Directory]::CreateDirectory($candidate) | Out-Null
    foreach ($entry in $archive.Entries) {
        $name = $entry.FullName.Replace('\', '/')
        if ($name -ceq $prefix) { continue }
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $candidate $name.Substring($prefix.Length)), $false)
    }
} finally { $archive.Dispose() }
Check-Hash 'DSH-Launcher.exe' (Join-Path $candidate 'DSH-Launcher.exe')
if ((Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $candidate 'runtime-manifest.json')).Hash -ine $manifest.runtime_manifest_sha256) {
    throw 'Runtime manifest checksum mismatch.'
}
