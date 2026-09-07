param([Parameter(Mandatory)][string]$LauncherPath, [Parameter(Mandatory)][string]$WorkDirectory,
      [Parameter(Mandatory)][string]$OldLauncherPath)
$ErrorActionPreference = 'Stop'
$source = (Resolve-Path -LiteralPath $LauncherPath).Path
$version = (Get-Item -LiteralPath $source).VersionInfo.ProductVersion
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Test requires an embedded release version.' }
$oldSource = (Resolve-Path -LiteralPath $OldLauncherPath).Path
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$work = [IO.Path]::GetFullPath($WorkDirectory)
if (!$work.StartsWith($repo + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Test directory must be in repository.' }
$files = @('DSH-Launcher.exe','runtime-manifest.json','dshctl.cmd','portable.flag')
function Wait-For([scriptblock]$Condition, [int]$Seconds = 40) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    while ([DateTime]::UtcNow -lt $deadline) { if (& $Condition) { return }; Start-Sleep -Milliseconds 50 }
    throw 'Timed out waiting for update fixture.'
}
function Stop-Fixture([string]$Root) {
    # A recovery helper can launch its replacement just as it exits.
    for ($attempt = 0; $attempt -lt 5; $attempt++) {
        Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($Root + '\', [StringComparison]::OrdinalIgnoreCase) } |
            ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
        Start-Sleep -Milliseconds 100
    }
}
$results = @()
foreach ($case in @('success','candidate-fails','file-locked','interrupted')) {
    $root = Join-Path $work $case
    $stage = Join-Path $root 'data\updates\launcher-update'
    $candidate = Join-Path $stage 'candidate'
    $backup = Join-Path $stage 'backup'
    foreach ($dir in @($root,$stage,$candidate,$backup,(Join-Path $root 'data\state'))) { [IO.Directory]::CreateDirectory($dir) | Out-Null }
    Copy-Item -LiteralPath $source -Destination (Join-Path $root 'DSH-Launcher.exe') -Force
    Copy-Item -LiteralPath (Join-Path $repo 'runtime-manifest.json') -Destination $root -Force
    [IO.File]::WriteAllText((Join-Path $root 'dshctl.cmd'),'old-cli')
    [IO.File]::WriteAllText((Join-Path $root 'portable.flag'),'')
    [IO.File]::WriteAllText((Join-Path $root 'data\state\sentinel'),'keep-user-data')
    foreach ($file in $files) { Copy-Item -LiteralPath (Join-Path $root $file) -Destination $backup -Force; Copy-Item -LiteralPath (Join-Path $root $file) -Destination $candidate -Force }
    [IO.File]::WriteAllText((Join-Path $candidate 'dshctl.cmd'),'new-cli')
    if ($case -eq 'candidate-fails') { Copy-Item -LiteralPath $oldSource -Destination (Join-Path $candidate 'DSH-Launcher.exe') -Force }
    $expectedHashes = @{}
    $expectedDirectory = if ($case -eq 'success') { $candidate } else { $backup }
    foreach ($file in $files) { $expectedHashes[$file] = (Get-FileHash -LiteralPath (Join-Path $expectedDirectory $file)).Hash }
    $recordFile = Join-Path $stage 'transaction.json'
    $record = @{ phase='prepared'; version=$version; token="fixture-$case"; parent_pid=0; parent_created=0; helper_pid=0; helper_created=0 }
    $lock = $null
    $parentId = 0
    try {
        if ($case -eq 'interrupted') {
            Copy-Item -LiteralPath (Join-Path $candidate 'dshctl.cmd') -Destination $root -Force
            $record.phase='committing'
            [IO.File]::WriteAllText($recordFile, ($record | ConvertTo-Json -Compress))
            Start-Process -FilePath (Join-Path $root 'DSH-Launcher.exe') -WindowStyle Hidden | Out-Null
        } else {
            $parent = Start-Process -FilePath (Join-Path $root 'DSH-Launcher.exe') -PassThru
            Wait-For { $parent.Refresh(); if ($parent.HasExited) { throw "Fixture parent exited: $case ($($parent.ExitCode))" }; $parent.MainWindowHandle -ne 0 }
            $parentId = $parent.Id
            $record.parent_pid=$parent.Id; $record.parent_created=$parent.StartTime.ToUniversalTime().ToFileTimeUtc()
            Copy-Item -LiteralPath $source -Destination (Join-Path $stage 'helper.exe') -Force
            [IO.File]::WriteAllText($recordFile, ($record | ConvertTo-Json -Compress))
            $helper = Start-Process -FilePath (Join-Path $stage 'helper.exe') -ArgumentList @('--self-update-apply', ('"'+$root+'"'), $record.token) -WindowStyle Hidden -PassThru
            $record.helper_pid=$helper.Id; $record.helper_created=$helper.StartTime.ToUniversalTime().ToFileTimeUtc()
            # Atomic record replacement, matching the production parent/helper handshake.
            $recordTemp=$recordFile+'.tmp'; [IO.File]::WriteAllText($recordTemp, ($record | ConvertTo-Json -Compress)); Move-Item -LiteralPath $recordTemp -Destination $recordFile -Force
            Wait-For { Test-Path -LiteralPath (Join-Path $stage 'helper-ready') }
            if ($case -eq 'file-locked') { $lock=[IO.File]::Open((Join-Path $root 'dshctl.cmd'),'Open','Read','Read') }
            Stop-Process -Id $parent.Id -Force
        }
        $expected = if ($case -eq 'success') { 'new-cli' } else { 'old-cli' }
        Wait-For {
            # The restored ordinary launcher legitimately removes a completed record.
            # Require its actual window, not just the helper's terminal phase.
            $replacement = @(Get-Process | Where-Object { $_.Path -eq (Join-Path $root 'DSH-Launcher.exe') -and $_.Id -ne $parentId -and $_.MainWindowHandle -ne 0 })
            if ($replacement.Count -ne 1) { return $false }
            if (Test-Path -LiteralPath $recordFile) {
                try { if ((Get-Content -Raw -LiteralPath $recordFile | ConvertFrom-Json).phase -notin @('done','restored')) { return $false } } catch { return $false }
            }
            return [IO.File]::ReadAllText((Join-Path $root 'dshctl.cmd')) -eq $expected
        }
        if ([IO.File]::ReadAllText((Join-Path $root 'dshctl.cmd')) -ne $expected) { throw "Wrong file group for $case" }
        foreach ($file in $files) {
            if ((Get-FileHash -LiteralPath (Join-Path $root $file)).Hash -ne $expectedHashes[$file]) { throw "Inconsistent program file for ${case}: $file" }
        }
        if ([IO.File]::ReadAllText((Join-Path $root 'data\state\sentinel')) -ne 'keep-user-data') { throw 'User data changed.' }
        $results += [ordered]@{case=$case;passed=$true;programGroup=$expected;programFilesVerified=4;windowReopened=$true;userDataPreserved=$true}
    } finally { if ($lock) { $lock.Dispose() }; Stop-Fixture $root }
}
$results | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $work 'results.json') -Encoding utf8
$results | Format-Table
