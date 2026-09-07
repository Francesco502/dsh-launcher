param([Parameter(Mandatory)][string]$LauncherPath,[Parameter(Mandatory)][string]$WorkDirectory)
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$exe=(Resolve-Path -LiteralPath $LauncherPath).Path
$work=[IO.Path]::GetFullPath($WorkDirectory)
if (!$work.StartsWith($repo+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Fixture must be inside repository.' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$files=@('DSH-Launcher.exe','runtime-manifest.json','dshctl.cmd','portable.flag')
$zipName='DSH-Launcher-Portable-x64.zip'
$results=@()
foreach($case in @('valid','offline','bad-checksum','wrong-version','wrong-architecture','zip-traversal','zip-duplicate')) {
    $fixture=Join-Path $work "$case\release"
    $destination=Join-Path $work "$case\download"
    [IO.Directory]::CreateDirectory($fixture) | Out-Null
    [IO.Directory]::CreateDirectory($destination) | Out-Null
    $zip=Join-Path $fixture $zipName
    $archive=[IO.Compression.ZipFile]::Open($zip,'Create')
    try {
        $archive.CreateEntry('DSH-Launcher-Portable-x64/') | Out-Null
        foreach($name in $files) {
            $entry=$archive.CreateEntry("DSH-Launcher-Portable-x64/$name")
            $stream=$entry.Open()
            try {
                $bytes=switch($name) {
                    'DSH-Launcher.exe' { [IO.File]::ReadAllBytes($exe) }
                    'runtime-manifest.json' { [IO.File]::ReadAllBytes((Join-Path $repo $name)) }
                    'dshctl.cmd' { [IO.File]::ReadAllBytes((Join-Path $PSScriptRoot $name)) }
                    default { [byte[]]@() }
                }
                if($bytes) { $stream.Write($bytes,0,$bytes.Length) }
            } finally { $stream.Dispose() }
        }
        if($case -eq 'zip-traversal') { $archive.CreateEntry('DSH-Launcher-Portable-x64/../../outside') | Out-Null }
        if($case -eq 'zip-duplicate') { $archive.CreateEntry('DSH-Launcher-Portable-x64/portable.flag') | Out-Null }
    } finally { $archive.Dispose() }
    $hash=(Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    $checksum=if($case -eq 'bad-checksum') { '0'*64 } else { $hash }
    [IO.File]::WriteAllText((Join-Path $fixture "$zipName.sha256"),"$checksum  $zipName")
    $manifest=@{schema_version=1;self_update_protocol=1;project='Francesco502/dsh-launcher';version='0.4.0';tag='v0.4.0';target='x86_64-pc-windows-gnu';architecture='x86_64';commit=('a'*40);
        runtime_manifest_sha256=(Get-FileHash -LiteralPath (Join-Path $repo 'runtime-manifest.json')).Hash.ToLowerInvariant();assets=@(
            @{name=$zipName;sha256=$hash},
            @{name="$zipName.sha256";sha256=(Get-FileHash -LiteralPath (Join-Path $fixture "$zipName.sha256")).Hash.ToLowerInvariant()},
            @{name='DSH-Launcher.exe';sha256=(Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant()})}
    if($case -eq 'wrong-version') { $manifest.version='9.9.9' }
    if($case -eq 'wrong-architecture') { $manifest.architecture='arm64' }
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $fixture 'release-manifest.json') -Encoding utf8
    function Invoke-WebRequest {
        param([switch]$UseBasicParsing,[int]$TimeoutSec,[string]$Uri,[string]$OutFile)
        if($case -eq 'offline') { throw 'Synthetic offline transport' }
        if(!$Uri.StartsWith('https://github.com/Francesco502/dsh-launcher/releases/download/v0.4.0/')) { throw 'Unexpected origin' }
        Copy-Item -LiteralPath (Join-Path $fixture ([Uri]$Uri).Segments[-1]) -Destination $OutFile
    }
    $failure=$null
    try { & (Join-Path $repo 'src\launcher_download.ps1') -Version '0.4.0' -Stage $destination } catch { $failure=$_.Exception.Message }
    if(($case -eq 'valid') -eq [bool]$failure) { throw "Unexpected result for ${case}: $failure" }
    $results += [ordered]@{case=$case;passed=$true;rejected=[bool]$failure;reason=$failure}
}
$results | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $work 'results.json') -Encoding utf8
$results | ConvertTo-Json
