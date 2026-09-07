param([Parameter(Mandatory)][int]$LauncherPid,[Parameter(Mandatory)][string]$OutputPath,[ValidateSet('visible','hidden')][string]$State,[int]$Seconds=600)
$ErrorActionPreference='Stop'
$process=Get-Process -Id $LauncherPid
$identity=$process.StartTime
$started=[DateTime]::UtcNow
$cpu=$process.TotalProcessorTime.TotalSeconds
$rows=@()
for($i=0;$i -lt $Seconds;$i++) {
    Start-Sleep -Seconds 1
    $process.Refresh()
    if($process.HasExited -or $process.StartTime -ne $identity) { throw 'Launcher changed during idle sample.' }
    if (($State -eq 'hidden') -ne ($process.MainWindowHandle -eq 0)) { throw 'Window visibility changed during idle sample.' }
    $rows += [ordered]@{seconds=([DateTime]::UtcNow-$started).TotalSeconds;workingSet=$process.WorkingSet64;privateBytes=$process.PrivateMemorySize64;handles=$process.HandleCount;threads=$process.Threads.Count}
}
$elapsed=([DateTime]::UtcNow-$started).TotalSeconds
$result=[ordered]@{state=$State;pid=$LauncherPid;durationSeconds=$elapsed;singleCoreCpuPercent=100*($process.TotalProcessorTime.TotalSeconds-$cpu)/$elapsed;
    workingSetMax=($rows.workingSet | Measure-Object -Maximum).Maximum;privateMax=($rows.privateBytes | Measure-Object -Maximum).Maximum;
    handlesMin=($rows.handles | Measure-Object -Minimum).Minimum;handlesMax=($rows.handles | Measure-Object -Maximum).Maximum;
    threadsMin=($rows.threads | Measure-Object -Minimum).Minimum;threadsMax=($rows.threads | Measure-Object -Maximum).Maximum;samples=$rows.Count}
$result | ConvertTo-Json | Set-Content -LiteralPath $OutputPath -Encoding utf8
$rows | ConvertTo-Json | Set-Content -LiteralPath ($OutputPath+'.samples.json') -Encoding utf8
$result | ConvertTo-Json
