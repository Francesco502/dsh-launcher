param([Parameter(Mandatory)][string]$AppDirectory, [Parameter(Mandatory)][string]$Baseline,
      [Parameter(Mandatory)][string]$Candidate, [int]$Cycles=20)
$ErrorActionPreference='Stop'
$app=(Resolve-Path -LiteralPath $AppDirectory).Path
$exe=Join-Path $app 'DSH-Launcher.exe'
$candidatePath=(Resolve-Path -LiteralPath $Candidate).Path
$baselinePath=(Resolve-Path -LiteralPath $Baseline).Path
$results=@()
function Invoke-Action([string]$Action) {
    $output=Join-Path $app "action-$Action.txt"
    $env:DSH_LAUNCHER_OUTPUT=$output
    $timer=[Diagnostics.Stopwatch]::StartNew()
    $process=Start-Process -FilePath $exe -ArgumentList @('--action',$Action) -WindowStyle Hidden -PassThru
    if (!$process.WaitForExit(90000)) { throw "$Action exceeded measurement budget" }
    $timer.Stop()
    $message=[IO.File]::ReadAllText($output)
    if ($process.ExitCode -ne 0) { throw "$Action failed: $message" }
    return $timer.Elapsed.TotalMilliseconds
}
try {
    foreach($variant in @(@{name='0.3.3';path=$baselinePath},@{name='0.4.0';path=$candidatePath})) {
        Copy-Item -LiteralPath $variant.path -Destination $exe -Force
        for($index=0;$index -le $Cycles;$index++) {
            $start=Invoke-Action 'start'
            $stop=Invoke-Action 'stop'
            if($index -gt 0) {
                $results += [ordered]@{version=$variant.name;cycle=$index;startMs=[math]::Round($start,2);stopMs=[math]::Round($stop,2)}
                $results | ConvertTo-Json | Set-Content -LiteralPath (Join-Path (Split-Path $app -Parent) 'lifecycle.json') -Encoding utf8
                Write-Output "$($variant.name) cycle $index start=$([math]::Round($start))ms stop=$([math]::Round($stop))ms"
            }
        }
    }
} finally {
    try { Invoke-Action 'stop' | Out-Null } catch { Write-Warning $_ }
    Copy-Item -LiteralPath $candidatePath -Destination $exe -Force
    Remove-Item Env:DSH_LAUNCHER_OUTPUT -ErrorAction SilentlyContinue
}
