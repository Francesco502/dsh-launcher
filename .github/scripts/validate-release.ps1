param(
    [Parameter(Mandatory = $true)]
    [string]$Tag
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

Push-Location $repositoryRoot
try {
    if ($Tag -notmatch '^v(?<version>\d+\.\d+\.\d+)$') {
        throw "Release tag '$Tag' must match vMAJOR.MINOR.PATCH."
    }
    $tagVersion = $Matches['version']

    $metadataOutput = & cargo metadata --locked --no-deps --format-version 1
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

    $lockText = Get-Content -Raw -LiteralPath 'Cargo.lock'
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

    $changelog = Get-Content -Raw -LiteralPath 'CHANGELOG.md'
    $changelogHeading = "(?m)^## \[$([regex]::Escape($package.version))\] - \d{4}-\d{2}-\d{2}\s*$"
    if ($changelog -notmatch $changelogHeading) {
        throw "CHANGELOG.md is missing a dated heading for version $($package.version)."
    }
    if ($changelog -notmatch '(?m)^## \[Unreleased\]\s*$') {
        throw "CHANGELOG.md must retain an Unreleased section."
    }

    Write-Output "Release metadata valid: $Tag ($($package.version))"
}
finally {
    Pop-Location
}
