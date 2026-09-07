param([Parameter(Mandatory)][string]$LauncherPath,[Parameter(Mandatory)][string]$WorkDirectory,[int]$Count=30)
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$root=[IO.Path]::GetFullPath($WorkDirectory)
if(!$root.StartsWith($repo+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Window fixture must be inside repository.' }
[IO.Directory]::CreateDirectory($root) | Out-Null
$exe=Join-Path $root 'DSH-Launcher.exe'
Copy-Item -LiteralPath $LauncherPath -Destination $exe -Force
Copy-Item -LiteralPath (Join-Path $repo 'runtime-manifest.json') -Destination $root -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'dshctl.cmd') -Destination $root -Force
[IO.File]::WriteAllText((Join-Path $root 'portable.flag'),'')
$package=Join-Path $root 'data\npm-global\node_modules\@deepseek-ai\dsh'
[IO.Directory]::CreateDirectory((Join-Path $package 'lib')) | Out-Null
[IO.File]::WriteAllText((Join-Path $package 'package.json'),'{"name":"@deepseek-ai/dsh","version":"1.0.0","dependencies":{}}')
[IO.File]::WriteAllText((Join-Path $package 'lib\bin.js'),'')
$samples=@()
for($index=0;$index -lt $Count+3;$index++) {
    $timer=[Diagnostics.Stopwatch]::StartNew()
    $process=Start-Process -FilePath $exe -PassThru
    try {
        do {
            $process.Refresh()
            if($process.HasExited -or $timer.Elapsed.TotalSeconds -gt 3) { throw 'First window unavailable.' }
            if($process.MainWindowHandle -ne 0) { break }
            Start-Sleep -Milliseconds 5
        } while($true)
        $timer.Stop()
        if($index -ge 3) { $samples+=$timer.Elapsed.TotalMilliseconds }
    } finally { Stop-Process -Id $process.Id -Force; $process.WaitForExit(3000) | Out-Null }
}
$sorted=@($samples | Sort-Object)
$result=[ordered]@{samples=$samples;count=$Count;warmups=3;p95Ms=$sorted[[math]::Ceiling($Count*0.95)-1];exeSha256=(Get-FileHash -LiteralPath $exe).Hash;method='Process creation to first visible main-window handle, 5 ms polling'}
$result | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'first-window.json') -Encoding utf8
$result | ConvertTo-Json
if($result.p95Ms -gt 500) { throw 'First window exceeded 500 ms gate.' }
