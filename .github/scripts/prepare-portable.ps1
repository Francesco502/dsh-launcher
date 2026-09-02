param(
    [Parameter(Mandatory = $true)]
    [string]$LauncherPath,

    [Parameter(Mandatory = $true)]
    [string]$OutputZip
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Test-PortableZipWhitelist {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ZipPath
    )

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($ZipPath)
    try {
        $entries = @($archive.Entries | ForEach-Object { $_.FullName.Replace('/', '\') })
        $prefix = "DSH-Launcher-Portable-x64\"
        $required = @(
            ($prefix + "DSH-Launcher.exe"),
            ($prefix + "portable.flag"),
            ($prefix + "runtime-manifest.json"),
            ($prefix + "dshctl.cmd")
        )
        if ($entries -notcontains $prefix) {
            throw "Portable ZIP is missing the required top-level directory."
        }
        $allowed = @($prefix) + $required
        foreach ($entry in $entries) {
            if ($allowed -notcontains $entry) {
                throw "Lightweight portable ZIP contains an unexpected entry: $entry"
            }
        }
        foreach ($requiredEntry in $required) {
            if ($entries -notcontains $requiredEntry) {
                throw "Portable ZIP is missing required entry: $requiredEntry"
            }
        }
    }
    finally {
        $archive.Dispose()
    }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repositoryRoot = (Resolve-Path (Join-Path $scriptRoot "..\..")).Path
$manifestPath = Join-Path $repositoryRoot "runtime-manifest.json"
if (-not (Test-Path -LiteralPath $LauncherPath -PathType Leaf)) {
    throw "Launcher executable does not exist: $LauncherPath"
}
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Runtime manifest does not exist: $manifestPath"
}

$runtimeManifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
if ($runtimeManifest.schema_version -ne 2 -or
    $runtimeManifest.architecture -ne "x86_64-pc-windows-gnu") {
    throw "Runtime manifest is not the Windows x64 schema 2 manifest."
}

$runnerTemp = if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) { $env:TEMP } else { $env:RUNNER_TEMP }
$workDirectory = Join-Path $runnerTemp "dsh-launcher-portable-$PID"
$portableDirectory = Join-Path $workDirectory "DSH-Launcher-Portable-x64"
$outputDirectory = Split-Path -Parent $OutputZip

try {
    New-Item -ItemType Directory -Path $portableDirectory -Force | Out-Null
    if (-not [string]::IsNullOrWhiteSpace($outputDirectory)) {
        New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
    }

    Copy-Item -LiteralPath $LauncherPath -Destination (Join-Path $portableDirectory "DSH-Launcher.exe") -Force
    Copy-Item -LiteralPath (Join-Path $scriptRoot "dshctl.cmd") -Destination (Join-Path $portableDirectory "dshctl.cmd") -Force
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $portableDirectory "runtime-manifest.json") -Force
    New-Item -ItemType File -Path (Join-Path $portableDirectory "portable.flag") -Force | Out-Null

    if (Test-Path -LiteralPath $OutputZip) {
        Remove-Item -LiteralPath $OutputZip -Force
    }
    $tarCommand = Get-Command tar.exe -ErrorAction SilentlyContinue
    if ($null -eq $tarCommand) {
        throw "Windows tar.exe is required to create the lightweight portable ZIP."
    }
    & $tarCommand.Source -a -c -f $OutputZip -C $workDirectory "DSH-Launcher-Portable-x64"
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to create lightweight portable ZIP: $OutputZip"
    }
    Test-PortableZipWhitelist -ZipPath $OutputZip
}
finally {
    if (Test-Path -LiteralPath $workDirectory) {
        Remove-Item -LiteralPath $workDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}
