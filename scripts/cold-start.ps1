# Bouncer's first launch on a fresh runner, nothing warmed: seconds to the
# pipe (PLAN, "The installer test's pipe failure"). CI only: it installs
# Bouncer. Warns above 30 s, fails above 120 s.
#
#   cold-start.ps1 -Setup <setup.exe>
#   cold-start.ps1 -Seconds <n>       (only the verdict, for the tests)
param([string]$Setup, [double]$Seconds = -1)
$ErrorActionPreference = "Stop"
$warn = 30
$limit = 120
$app = $null

if ($Seconds -lt 0) {
    if (-not $env:GITHUB_ACTIONS) { throw "CI only: this installs Bouncer" }
    $p = Start-Process $Setup -ArgumentList "/S" -PassThru -Wait
    if ($p.ExitCode) { throw "the installer exited $($p.ExitCode)" }
    $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $app = Start-Process (Join-Path $env:LOCALAPPDATA "Bouncer\bouncer-app.exe") -PassThru
    while ($sw.Elapsed.TotalSeconds -lt $limit -and [System.IO.Directory]::GetFiles("\\.\pipe\") -notcontains "\\.\pipe\bouncer-$sid") {
        Start-Sleep -Milliseconds 250
    }
    $Seconds = $sw.Elapsed.TotalSeconds
}

$verdict = if ($Seconds -gt $limit) { "FAIL: no pipe within $limit s" } elseif ($Seconds -gt $warn) { "WARN: over $warn s" } else { "ok" }
$webview = (Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application" -ErrorAction SilentlyContinue | Where-Object Name -match '^\d').Name -join ", "
$line = "Cold start: {0:n1} s to the pipe, {1} (image {2}, WebView2 {3})" -f $Seconds, $verdict, $env:ImageVersion, $webview
Write-Host $line
if ($env:GITHUB_STEP_SUMMARY) { $line | Add-Content $env:GITHUB_STEP_SUMMARY -Encoding utf8 }
if ($verdict -ne "ok") { Write-Host "::warning::$line" }
if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
if ($Seconds -gt $limit) { exit 1 }
