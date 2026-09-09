param(
    [Parameter(Mandatory = $true)]
    [string]$OutputPath
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

function New-SpdxId {
    param([string]$Value)

    return "SPDXRef-" + (($Value -replace '[^A-Za-z0-9.-]', '-') -replace '-+', '-')
}

$metadataOutput = & cargo metadata --manifest-path (Join-Path $repositoryRoot "Cargo.toml") --locked --format-version 1
if ($LASTEXITCODE -ne 0) {
    throw "cargo metadata failed with exit code $LASTEXITCODE."
}
$metadata = ($metadataOutput -join [Environment]::NewLine) | ConvertFrom-Json
$application = @($metadata.packages | Where-Object { $_.name -eq 'dsh-launcher' }) | Select-Object -First 1
if ($null -eq $application) {
    throw "Cargo package dsh-launcher was not found."
}
$applicationVersion = [string]$application.version
$runtimeManifest = Get-Content -Raw -LiteralPath (Join-Path $repositoryRoot "runtime-manifest.json") | ConvertFrom-Json

$packages = @()
foreach ($package in @($metadata.packages)) {
    $packages += [ordered]@{
        SPDXID = New-SpdxId "cargo-$($package.name)-$($package.version)"
        name = "cargo:$($package.name)"
        versionInfo = $package.version
        downloadLocation = if ($package.source) { $package.source } else { "NOASSERTION" }
        filesAnalyzed = $false
        licenseDeclared = if ($package.license -and $package.license -notmatch 'LicenseRef-') { $package.license } else { 'NOASSERTION' }
        licenseConcluded = if ($package.license -match 'LicenseRef-Slint-Royalty-free-2.0') { 'LicenseRef-Slint-Royalty-free-2.0' } else { 'NOASSERTION' }
        licenseComments = if ($package.license) { "Cargo declared license alternatives: $($package.license)" } else { 'Consult the package license file.' }
    }
}

$packages += [ordered]@{
    SPDXID = New-SpdxId "runtime-dsh-selected-at-install-time"
    name = "npm:$($runtimeManifest.dsh.package)"
    versionInfo = "selected-at-install-time"
    downloadLocation = [string]$runtimeManifest.dsh.registry
    filesAnalyzed = $false
}

$manifest = [ordered]@{
    SPDXID = "SPDXRef-DOCUMENT"
    spdxVersion = "SPDX-2.3"
    dataLicense = "CC0-1.0"
    name = "DSH-Launcher-$applicationVersion"
    documentNamespace = "https://github.com/Francesco502/dsh-launcher/spdx/DSH-Launcher-$applicationVersion"
    creationInfo = [ordered]@{
        created = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
        creators = @( "Tool: DSH Launcher prepare-sbom.ps1" )
    }
    packages = $packages
    comment = 'Cargo dependency graph includes build-time and target-specific packages; the separately listed DSH runtime is selected at installation and is not bundled.'
    hasExtractedLicensingInfos = @([ordered]@{
        licenseId = 'LicenseRef-Slint-Royalty-free-2.0'
        name = 'Slint Royalty-free License 2.0'
        extractedText = Get-Content -Raw -LiteralPath (Join-Path $repositoryRoot 'licenses/Slint-Royalty-free-2.0.md')
        seeAlsos = @('https://slint.dev/license')
    })
}
$parent = Split-Path -Parent $OutputPath
New-Item -ItemType Directory -Path $parent -Force | Out-Null
$manifest | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $OutputPath -Encoding utf8
