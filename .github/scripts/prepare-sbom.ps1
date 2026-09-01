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
$peerDependencies = @($runtimeManifest.dsh.peer_dependencies)

$packages = @()
foreach ($package in @($metadata.packages)) {
    $packages += [ordered]@{
        SPDXID = New-SpdxId "cargo-$($package.name)-$($package.version)"
        name = "cargo:$($package.name)"
        versionInfo = $package.version
        downloadLocation = if ($package.source) { $package.source } else { "NOASSERTION" }
        filesAnalyzed = $false
    }
}

foreach ($component in @(
    [ordered]@{
        id = "nodejs-$($runtimeManifest.node.version)"
        name = "runtime:Node.js"
        version = [string]$runtimeManifest.node.version
        location = [string]$runtimeManifest.node.url
        sha256 = [string]$runtimeManifest.node.sha256
    },
    [ordered]@{
        id = "dsh-$($runtimeManifest.dsh.bootstrap_version)"
        name = "npm:$($runtimeManifest.dsh.package)"
        version = [string]$runtimeManifest.dsh.bootstrap_version
        location = [string]$runtimeManifest.dsh.registry_url
        sha256 = $null
    },
    [ordered]@{
        id = "dsh-quota-$($runtimeManifest.quota.version)"
        name = "npm:$($runtimeManifest.quota.package)"
        version = [string]$runtimeManifest.quota.version
        location = [string]$runtimeManifest.quota.url
        sha256 = [string]$runtimeManifest.quota.sha256
    }
)) {
    $packages += [ordered]@{
        SPDXID = New-SpdxId "runtime-$($component.id)"
        name = $component.name
        versionInfo = $component.version
        downloadLocation = $component.location
        filesAnalyzed = $false
        checksums = if ($component.sha256) {
            @(
                [ordered]@{
                    algorithm = "SHA256"
                    checksumValue = $component.sha256
                }
            )
        } else { @() }
    }
}

foreach ($spec in $peerDependencies) {
    $separator = ([string]$spec).LastIndexOf('@')
    if ($separator -le 0 -or $separator -ge ([string]$spec).Length - 1) {
        throw "Invalid fixed DSH peer dependency spec: $spec"
    }
    $peerName = ([string]$spec).Substring(0, $separator)
    $peerVersion = ([string]$spec).Substring($separator + 1)
    $packages += [ordered]@{
        SPDXID = New-SpdxId "npm-peer-$spec"
        name = "npm:$peerName"
        versionInfo = $peerVersion
        downloadLocation = "https://registry.npmjs.org/$([uri]::EscapeDataString($peerName))"
        filesAnalyzed = $false
    }
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
}
$parent = Split-Path -Parent $OutputPath
New-Item -ItemType Directory -Path $parent -Force | Out-Null
$manifest | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $OutputPath -Encoding utf8
