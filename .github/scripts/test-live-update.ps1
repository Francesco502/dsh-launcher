param([Parameter(Mandatory)][string]$AppDirectory,[Parameter(Mandatory)][string]$Candidate)
$ErrorActionPreference='Stop'
$root=(Resolve-Path -LiteralPath $AppDirectory).Path
$repo=Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
if(!$root.StartsWith($repo+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Live update test requires an isolated repository directory.' }
$exe=Join-Path $root 'DSH-Launcher.exe'
$parent=@(Get-Process | Where-Object Path -eq $exe)
if($parent.Count -ne 1) { throw 'Expected one test launcher window.' }
$parent=$parent[0]
$worker=[IO.File]::ReadAllText((Join-Path $root 'data\state\dsh.pid'))
$listener=(Get-NetTCPConnection -State Listen -LocalPort 3080).OwningProcess
$settings=Get-FileHash -LiteralPath (Join-Path $root 'data\state\plugin-settings.json')
$helperHash=Get-FileHash -LiteralPath (Join-Path $root 'data\state\log-worker.exe')
$stage=Join-Path $root 'data\updates\launcher-update'
$files=@('DSH-Launcher.exe','runtime-manifest.json','dshctl.cmd','portable.flag')
foreach($dir in @($stage,(Join-Path $stage 'candidate'),(Join-Path $stage 'backup'))) { [IO.Directory]::CreateDirectory($dir) | Out-Null }
foreach($name in $files) { Copy-Item -LiteralPath (Join-Path $root $name) -Destination (Join-Path $stage 'backup') -Force; Copy-Item -LiteralPath (Join-Path $root $name) -Destination (Join-Path $stage 'candidate') -Force }
Copy-Item -LiteralPath $Candidate -Destination (Join-Path $stage 'candidate\DSH-Launcher.exe') -Force
Copy-Item -LiteralPath $exe -Destination (Join-Path $stage 'helper.exe') -Force
$record=@{phase='prepared';version='0.4.0';token='live-update-test';parent_pid=$parent.Id;parent_created=$parent.StartTime.ToUniversalTime().ToFileTimeUtc();helper_pid=0;helper_created=0}
$recordFile=Join-Path $stage 'transaction.json'
[IO.File]::WriteAllText($recordFile,($record | ConvertTo-Json -Compress))
$helper=Start-Process -FilePath (Join-Path $stage 'helper.exe') -ArgumentList @('--self-update-apply',('"'+$root+'"'),$record.token) -WindowStyle Hidden -PassThru
$record.helper_pid=$helper.Id; $record.helper_created=$helper.StartTime.ToUniversalTime().ToFileTimeUtc()
$temporary=$recordFile+'.tmp'; [IO.File]::WriteAllText($temporary,($record | ConvertTo-Json -Compress)); Move-Item -LiteralPath $temporary -Destination $recordFile -Force
$deadline=[DateTime]::UtcNow.AddSeconds(15)
while(!(Test-Path -LiteralPath (Join-Path $stage 'helper-ready'))) { if([DateTime]::UtcNow -gt $deadline) { throw 'Helper not ready' }; Start-Sleep -Milliseconds 50 }
Stop-Process -Id $parent.Id -Force
$deadline=[DateTime]::UtcNow.AddSeconds(40)
do { Start-Sleep -Milliseconds 50; $phase=(Get-Content -Raw -LiteralPath $recordFile | ConvertFrom-Json).phase; if([DateTime]::UtcNow -gt $deadline) { throw 'Update did not complete' } } while($phase -notin @('done','restored'))
if($phase -ne 'done') { throw 'Live update rolled back' }
foreach($name in $files) { if((Get-FileHash -LiteralPath (Join-Path $root $name)).Hash -ne (Get-FileHash -LiteralPath (Join-Path $stage "candidate\$name")).Hash) { throw "Mixed program group: $name" } }
$result=[ordered]@{phase=$phase;oldLauncherPid=$parent.Id;newLauncherPid=(Get-Content -Raw -LiteralPath (Join-Path $stage 'window-ready') | ConvertFrom-Json).pid;
    workerUnchanged=($worker -eq [IO.File]::ReadAllText((Join-Path $root 'data\state\dsh.pid')));
    portOwnerUnchanged=($listener -eq (Get-NetTCPConnection -State Listen -LocalPort 3080).OwningProcess);
    settingsUnchanged=($settings.Hash -eq (Get-FileHash -LiteralPath (Join-Path $root 'data\state\plugin-settings.json')).Hash);
    logWorkerUnchanged=($helperHash.Hash -eq (Get-FileHash -LiteralPath (Join-Path $root 'data\state\log-worker.exe')).Hash)}
if($result.Values -contains $false) { throw 'DSH continuity check failed' }
$result | ConvertTo-Json | Set-Content -LiteralPath (Join-Path (Split-Path $root -Parent) 'live-update.json') -Encoding utf8
$result | ConvertTo-Json
