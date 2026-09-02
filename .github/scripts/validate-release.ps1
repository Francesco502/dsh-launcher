param(
    [Parameter(Mandatory = $true)]
    [string]$Tag,

    [Parameter(Mandatory = $false)]
    [string]$ReleaseDirectory,

    [Parameter(Mandatory = $false)]
    [switch]$AllowLocalWorkingTree
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

if ($Tag -notmatch '^v(?<version>\d+\.\d+\.\d+)$') {
    throw "Release tag '$Tag' must match vMAJOR.MINOR.PATCH."
}
$tagVersion = $Matches['version']

$metadataOutput = & cargo metadata --manifest-path (Join-Path $repositoryRoot 'Cargo.toml') --locked --format-version 1
if ($LASTEXITCODE -ne 0) {
    throw "cargo metadata failed with exit code $LASTEXITCODE."
}
$metadata = ($metadataOutput -join [Environment]::NewLine) | ConvertFrom-Json
$package = @($metadata.packages | Where-Object { $_.name -eq 'dsh-launcher' }) | Select-Object -First 1
if ($null -eq $package) {
    throw "Cargo package dsh-launcher was not found."
}
if ($package.version -ne $tagVersion) {
    throw "Git tag $Tag does not match Cargo package version $($package.version)."
}

$lockText = Get-Content -Raw -LiteralPath (Join-Path $repositoryRoot 'Cargo.lock')
    $lockMatch = [regex]::Match(
        $lockText,
        '(?ms)^\[\[package\]\]\s*\r?\nname = "dsh-launcher"\s*\r?\nversion = "([^"]+)"'
    )
    if (-not $lockMatch.Success) {
        throw "Cargo.lock does not contain the dsh-launcher package entry."
    }
    if ($lockMatch.Groups[1].Value -ne $package.version) {
        throw "Cargo.lock dsh-launcher version $($lockMatch.Groups[1].Value) does not match Cargo.toml $($package.version)."
    }

$changelog = Get-Content -Raw -LiteralPath (Join-Path $repositoryRoot 'CHANGELOG.md')
    $changelogHeading = "(?m)^## \[$([regex]::Escape($package.version))\] - \d{4}-\d{2}-\d{2}\s*$"
    if ($changelog -notmatch $changelogHeading) {
        throw "CHANGELOG.md is missing a dated heading for version $($package.version)."
    }
    if ($changelog -notmatch '(?m)^## \[Unreleased\]\s*$') {
        throw "CHANGELOG.md must retain an Unreleased section."
    }

$runtimeManifest = Get-Content -Raw -LiteralPath (Join-Path $repositoryRoot 'runtime-manifest.json') | ConvertFrom-Json
    if ($runtimeManifest.schema_version -ne 2 -or
        $runtimeManifest.architecture -ne 'x86_64-pc-windows-gnu' -or
        $runtimeManifest.dsh.package -ne '@deepseek-ai/dsh' -or
        $runtimeManifest.dsh.registry -ne 'https://registry.npmjs.org/' -or
        $runtimeManifest.dsh.entry -ne 'lib/bin.js' -or
        $runtimeManifest.node.download_page -ne 'https://nodejs.org/en/download') {
        throw "runtime-manifest.json is not the Windows x64 schema 2 lightweight manifest."
    }

    if (-not [string]::IsNullOrWhiteSpace($ReleaseDirectory)) {
        $releasePath = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
        $expectedAssetNames = @(
            'DSH-Launcher.exe',
            'DSH-Launcher.exe.sha256',
            'DSH-Launcher-Portable-x64.zip',
            'DSH-Launcher-Portable-x64.zip.sha256',
            'release-manifest.json',
            'sbom.spdx.json'
        ) | Sort-Object
        $actualAssetNames = @(Get-ChildItem -LiteralPath $releasePath -File | Select-Object -ExpandProperty Name | Sort-Object)
        if (($actualAssetNames -join '|') -ne ($expectedAssetNames -join '|')) {
            throw "Release asset set is not exactly the required six files: $($actualAssetNames -join ', ')"
        }

        function Assert-ChecksumAsset {
            param(
                [string]$ChecksumPath,
                [string]$AssetName,
                [string]$AssetPath
            )
            $checksum = (Get-Content -Raw -LiteralPath $ChecksumPath).Trim()
            if ($checksum -notmatch '^(?<hash>[0-9a-fA-F]{64})\s+(?<name>.+)$') {
                throw "Checksum file has an invalid format: $ChecksumPath"
            }
            if ([IO.Path]::GetFileName($Matches['name'].Trim()) -ne $AssetName) {
                throw "Checksum file names the wrong asset: $ChecksumPath"
            }
            $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $AssetPath).Hash.ToLowerInvariant()
            if ($actual -ne $Matches['hash'].ToLowerInvariant()) {
                throw "Checksum mismatch for $AssetName."
            }
        }

        Assert-ChecksumAsset `
            -ChecksumPath (Join-Path $releasePath 'DSH-Launcher.exe.sha256') `
            -AssetName 'DSH-Launcher.exe' `
            -AssetPath (Join-Path $releasePath 'DSH-Launcher.exe')
        Assert-ChecksumAsset `
            -ChecksumPath (Join-Path $releasePath 'DSH-Launcher-Portable-x64.zip.sha256') `
            -AssetName 'DSH-Launcher-Portable-x64.zip' `
            -AssetPath (Join-Path $releasePath 'DSH-Launcher-Portable-x64.zip')
        $versionInfo = (Get-Item -LiteralPath (Join-Path $releasePath 'DSH-Launcher.exe')).VersionInfo
        if ($versionInfo.FileDescription -ne 'DSH启动器' -or
            $versionInfo.ProductName -ne 'DSH启动器' -or
            $versionInfo.OriginalFilename -ne 'DSH-Launcher.exe' -or
            $versionInfo.FileVersion -ne $package.version -or
            $versionInfo.ProductVersion -ne $package.version) {
            throw 'DSH-Launcher.exe VERSIONINFO is missing or inconsistent with Cargo metadata.'
        }
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zipPath = Join-Path $releasePath 'DSH-Launcher-Portable-x64.zip'
        $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
        try {
            $entries = @($archive.Entries | ForEach-Object { $_.FullName.Replace('/', '\') })
            $prefix = 'DSH-Launcher-Portable-x64\'
            $requiredEntries = @(
                $prefix,
                ($prefix + 'DSH-Launcher.exe'),
                ($prefix + 'portable.flag'),
                ($prefix + 'runtime-manifest.json'),
                ($prefix + 'dshctl.cmd')
            )
            foreach ($entry in $entries) {
                if ($requiredEntries -notcontains $entry) {
                    throw "Lightweight portable ZIP contains an unexpected entry: $entry"
                }
            }
            foreach ($requiredEntry in $requiredEntries) {
                if ($entries -notcontains $requiredEntry) {
                    throw "Portable ZIP is missing required entry: $requiredEntry"
                }
            }
            if ($entries.Count -ne $requiredEntries.Count) {
                throw 'Lightweight portable ZIP contains duplicate or unexpected entries.'
            }
        }
        finally {
            $archive.Dispose()
        }
        $lockHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repositoryRoot 'Cargo.lock')).Hash.ToLowerInvariant()

        $manifestPath = Join-Path $releasePath 'release-manifest.json'
        $releaseManifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
        if ($releaseManifest.schema_version -ne 1 -or
            $releaseManifest.project -ne 'Francesco502/dsh-launcher' -or
            $releaseManifest.version -ne $package.version -or
            $releaseManifest.tag -ne $Tag -or
            $releaseManifest.target -ne 'x86_64-pc-windows-gnu' -or
            $releaseManifest.architecture -ne 'x86_64' -or
            $releaseManifest.authenticode_status -ne 'unsigned') {
            throw 'release-manifest.json does not match the release contract.'
        }
        $manifestCommit = [string]$releaseManifest.commit
        if ($AllowLocalWorkingTree -and $manifestCommit -eq 'local-working-tree') {
            # Local staging may describe an uncommitted tree only when the
            # caller explicitly opts into this non-release validation mode.
        } elseif ($manifestCommit -notmatch '^[0-9a-fA-F]{40}$') {
            throw 'release-manifest.json commit must be a 40-character commit SHA.'
        } else {
            $tagCommit = (& git -C $repositoryRoot rev-parse "$Tag`^{commit}" 2>$null).Trim()
            if ($LASTEXITCODE -eq 0 -and $tagCommit -ne $manifestCommit.ToLowerInvariant()) {
                throw "release-manifest.json commit $manifestCommit does not match tag $Tag commit $tagCommit."
            }
        }
        $runtimeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repositoryRoot 'runtime-manifest.json')).Hash.ToLowerInvariant()
        if ($releaseManifest.runtime_manifest_sha256 -ne $runtimeHash -or
            $releaseManifest.dependency_lock_sha256 -ne $lockHash) {
            throw 'release-manifest.json runtime or dependency lock hash is incorrect.'
        }
        $manifestAssetEntries = @($releaseManifest.assets)
        $expectedManifestEntries = @(
            'DSH-Launcher.exe',
            'DSH-Launcher.exe.sha256',
            'DSH-Launcher-Portable-x64.zip',
            'DSH-Launcher-Portable-x64.zip.sha256',
            'sbom.spdx.json'
        ) | Sort-Object
        $actualManifestEntries = @($manifestAssetEntries | Select-Object -ExpandProperty name | Sort-Object)
        if (($actualManifestEntries -join '|') -ne ($expectedManifestEntries -join '|')) {
            throw 'release-manifest.json asset list is incomplete or contains unexpected files.'
        }
        foreach ($asset in $manifestAssetEntries) {
            $assetPath = Join-Path $releasePath $asset.name
            $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $assetPath).Hash.ToLowerInvariant()
            if ($actual -ne ([string]$asset.sha256).ToLowerInvariant()) {
                throw "release-manifest.json hash mismatch for $($asset.name)."
            }
        }
        $sbom = Get-Content -Raw -LiteralPath (Join-Path $releasePath 'sbom.spdx.json') | ConvertFrom-Json
        if ($sbom.spdxVersion -ne 'SPDX-2.3' -or $null -eq $sbom.packages) {
            throw 'SBOM is not a valid SPDX 2.3 document.'
        }
        if (-not @($sbom.packages | Where-Object { $_.name -eq 'npm:@deepseek-ai/dsh' })) {
            throw 'SBOM is missing the runtime-selected DSH package declaration.'
        }
        foreach ($cargoPackage in @($metadata.packages)) {
            $sbomCargoName = "cargo:$($cargoPackage.name)"
            if (-not @($sbom.packages | Where-Object {
                    $_.name -eq $sbomCargoName -and $_.versionInfo -eq $cargoPackage.version
                })) {
                throw "SBOM is missing Cargo dependency: $($cargoPackage.name) $($cargoPackage.version)"
            }
        }
    }

Write-Output "Release metadata valid: $Tag ($($package.version))"
