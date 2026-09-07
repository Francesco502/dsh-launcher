param([Parameter(Mandatory)][string]$AppDirectory,[Parameter(Mandatory)][string]$NodePath)
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$app=(Resolve-Path -LiteralPath $AppDirectory).Path
if(!$app.StartsWith($repo+'\.tmp-',[StringComparison]::OrdinalIgnoreCase)) { throw 'Requires an independent repository test installation.' }
$exe=Join-Path $app 'DSH-Launcher.exe'
$output=Join-Path $app 'port-conflict-output.txt'
$savedOutput=$env:DSH_LAUNCHER_OUTPUT
$env:DSH_LAUNCHER_OUTPUT=$output
function Invoke-Action([string]$Action) {
    $child=Start-Process -FilePath $exe -ArgumentList @('--action',$Action) -WindowStyle Hidden -PassThru
    if(!$child.WaitForExit(20000)) { throw "Action timeout: $Action" }
    return @{exit=$child.ExitCode;text=[IO.File]::ReadAllText($output)}
}
$fixture=Join-Path $app 'unrelated-port-owner.cjs'
$ready=Join-Path $app 'unrelated-port-owner.ready'
$owner=$null
try {
    $stopped=Invoke-Action 'stop'
    if($stopped.exit -ne 0) { throw $stopped.text }
    [IO.File]::WriteAllText($fixture,"require('node:net').createServer().listen(3080,'127.0.0.1',()=>require('node:fs').writeFileSync(__filename+'.ready','ready'));setTimeout(()=>process.exit(0),60000);")
    $ready=$fixture+'.ready'
    if(Test-Path -LiteralPath $ready) { Remove-Item -LiteralPath $ready }
    $owner=Start-Process -FilePath $NodePath -ArgumentList ('"'+$fixture+'"') -WindowStyle Hidden -PassThru
    $deadline=[DateTime]::UtcNow.AddSeconds(5)
    while(!(Test-Path -LiteralPath $ready)) { if([DateTime]::UtcNow -gt $deadline) { throw 'Port fixture did not start.' }; Start-Sleep -Milliseconds 20 }
    $start=Invoke-Action 'start'
    if($start.exit -eq 0 -or $start.text -notmatch '3080') { throw "Conflict was not reported: $($start.text)" }
    $stop=Invoke-Action 'stop'
    $owner.Refresh()
    if($owner.HasExited) { throw 'Launcher killed an unrelated port owner.' }
    if(Test-Path -LiteralPath (Join-Path $app 'data/state/dsh-repair-needed')) { throw 'Port conflict incorrectly requested reinstall.' }
    [ordered]@{startRejected=$true;unrelatedOwnerPreserved=$true;repairMarkerAbsent=$true;startMessage=$start.text;stopMessage=$stop.text} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $app 'port-conflict-result.json') -Encoding utf8
} finally {
    if($owner -and !$owner.HasExited) { Stop-Process -Id $owner.Id -Force; $owner.WaitForExit(5000) | Out-Null }
    $env:DSH_LAUNCHER_OUTPUT=$savedOutput
}
