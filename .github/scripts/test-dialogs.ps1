param([Parameter(Mandatory)][string]$Target)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$build = & cargo test --manifest-path (Join-Path $repo 'Cargo.toml') --locked --target $Target --no-run --message-format=json
if ($LASTEXITCODE -ne 0) { throw 'Dialog test build failed.' }
$binary = @($build | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object {
    $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'dsh-launcher' -and $_.profile.test -and $_.executable
} | Select-Object -ExpandProperty executable)
if ($binary.Count -ne 1) { throw 'Expected exactly one launcher test executable.' }
$output = Join-Path $repo ".tmp-dialog-tests\$Target"
[IO.Directory]::CreateDirectory($output) | Out-Null
foreach ($test in @('slint_window_lifetime_and_opaque_states')) {
    $stdout = Join-Path $output "$test.out"
    $stderr = Join-Path $output "$test.err"
    $process = Start-Process -FilePath $binary[0] -ArgumentList @($test,'--ignored','--nocapture','--test-threads=1') -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
    if (!$process.WaitForExit(60000)) {
        Stop-Process -Id $process.Id -Force
        throw "$test exceeded its 60-second watchdog."
    }
    Get-Content -LiteralPath $stdout
    Get-Content -LiteralPath $stderr
    if ($process.ExitCode -ne 0) { throw "$test failed ($($process.ExitCode))." }
}
