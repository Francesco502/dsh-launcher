param([Parameter(Mandatory)][int]$LauncherPid,[Parameter(Mandatory)][string]$OutputPath,[ValidateSet('visible','hidden')][string]$State,[int]$Seconds=60)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'launcher-window.ps1')
$process=Get-Process -Id $LauncherPid
$identity=$process.StartTime
$started=[DateTime]::UtcNow
$cpu=$process.TotalProcessorTime.TotalSeconds
$rows=@()
for($i=0;$i -lt $Seconds;$i++) {
    Start-Sleep -Seconds 1
    $process.Refresh()
    if($process.HasExited -or $process.StartTime -ne $identity) { throw 'Launcher changed during idle sample.' }
    # winit keeps an unnamed Thread Event Target while the tray is alive.
    # MainWindowHandle can select that helper after the actual panel is destroyed.
    $panels = [LauncherWindowProbe]::Count($LauncherPid)
    if ($panels -gt 1) { throw 'Duplicate launcher panels.' }
    $panelVisible = $panels -eq 1
    if (($State -eq 'visible') -ne $panelVisible) { throw "Panel visibility changed: state=$State, title=$($process.MainWindowTitle)." }
    $counter=Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$LauncherPid"
    if ($null -eq $counter) { throw 'Private working set counter unavailable.' }
    $rows += [ordered]@{seconds=([DateTime]::UtcNow-$started).TotalSeconds;privateWorkingSet=[long]$counter.WorkingSetPrivate;workingSet=$process.WorkingSet64;peakWorkingSet=$process.PeakWorkingSet64;privateBytes=$process.PrivateMemorySize64;handles=$process.HandleCount;threads=$process.Threads.Count}
}
$elapsed=([DateTime]::UtcNow-$started).TotalSeconds
$result=[ordered]@{state=$State;pid=$LauncherPid;durationSeconds=$elapsed;singleCoreCpuPercent=100*($process.TotalProcessorTime.TotalSeconds-$cpu)/$elapsed;
    wholeMachineCpuPercent=100*($process.TotalProcessorTime.TotalSeconds-$cpu)/$elapsed/[Environment]::ProcessorCount;
    privateWorkingSetMax=($rows.privateWorkingSet | Measure-Object -Maximum).Maximum;
    workingSetMax=($rows.workingSet | Measure-Object -Maximum).Maximum;privateMax=($rows.privateBytes | Measure-Object -Maximum).Maximum;
    handlesMin=($rows.handles | Measure-Object -Minimum).Minimum;handlesMax=($rows.handles | Measure-Object -Maximum).Maximum;
    threadsMin=($rows.threads | Measure-Object -Minimum).Minimum;threadsMax=($rows.threads | Measure-Object -Maximum).Maximum;samples=$rows.Count}
$result | ConvertTo-Json | Set-Content -LiteralPath $OutputPath -Encoding utf8
$rows | ConvertTo-Json | Set-Content -LiteralPath ($OutputPath+'.samples.json') -Encoding utf8
$result | ConvertTo-Json
