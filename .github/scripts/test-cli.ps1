param(
    [Parameter(Mandatory = $true)]
    [string]$LauncherPath
)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
$fixture = Join-Path $tempBase ('dsh-cli-test-' + [Guid]::NewGuid().ToString('N'))
$utf8 = [Text.UTF8Encoding]::new($false, $true)

function Invoke-CliCheck {
    param([string]$Program, [string]$Arguments, [int]$ExpectedCode, [string]$ExpectedText)
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Program
    $info.Arguments = $Arguments
    $info.WorkingDirectory = $fixture
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.Environment.Remove('DSH_LAUNCHER_OUTPUT') | Out-Null
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    $stdout = [IO.MemoryStream]::new()
    $stderr = [IO.MemoryStream]::new()
    try {
        if (-not $process.Start()) { throw "Could not start CLI: $Arguments" }
        $outTask = $process.StandardOutput.BaseStream.CopyToAsync($stdout)
        $errTask = $process.StandardError.BaseStream.CopyToAsync($stderr)
        if (-not $process.WaitForExit(15000)) {
            $process.Kill($true)
            throw "CLI timed out: $Arguments"
        }
        [void]$outTask.GetAwaiter().GetResult()
        [void]$errTask.GetAwaiter().GetResult()
        $output = $utf8.GetString($stdout.ToArray()) + $utf8.GetString($stderr.ToArray())
        if ($process.ExitCode -ne $ExpectedCode -or $output -notmatch $ExpectedText -or $output.Contains('????')) {
            throw "CLI failed: $Arguments; exit=$($process.ExitCode); output=$output"
        }
        Write-Output "CLI UTF-8 and exit code passed: $Arguments (exit $ExpectedCode)"
    } finally {
        $process.Dispose()
        $stdout.Dispose()
        $stderr.Dispose()
    }
}

try {
    New-Item -ItemType Directory -Path $fixture | Out-Null
    $launcher = Join-Path $fixture 'DSH-Launcher.exe'
    Copy-Item -LiteralPath $LauncherPath -Destination $launcher
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'dshctl.cmd') -Destination $fixture
    Invoke-CliCheck $launcher '--action invalid' 2 '未知操作'
    Invoke-CliCheck "$env:SystemRoot\System32\cmd.exe" '/d /c dshctl.cmd invalid' 2 '未知操作'
    # Without portable.flag, every public action must fail before touching DSH.
    # An elevated CI runner instead exits at the non-admin requirement.
    foreach ($action in @('start', 'stop', 'upgrade', 'open')) {
        Invoke-CliCheck "$env:SystemRoot\System32\cmd.exe" "/d /c dshctl.cmd $action" 1 '请'
    }
    Copy-Item -LiteralPath (Join-Path $repo 'runtime-manifest.json') -Destination $fixture
    New-Item -ItemType File -Path (Join-Path $fixture 'portable.flag') | Out-Null
    Invoke-CliCheck $launcher '--release-smoke' 0 '初始化检查通过'
} finally {
    if (Test-Path -LiteralPath $fixture) {
        $resolved = (Resolve-Path -LiteralPath $fixture).Path
        if (-not $resolved.StartsWith($tempBase + '\', [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Refusing to remove a CLI fixture outside its temporary root.'
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
