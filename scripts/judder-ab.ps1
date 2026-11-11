# Compare how smooth each Windows capture path records the same game.
#
# One run records one path: it stops any running Reruns, switches the
# capture settings, starts the build under target\release, waits for the
# clip you save with the hotkey, and scores it with the judder example.
# Leave the game running between runs and play the same scene each time.
#
#   pwsh scripts/judder-ab.ps1 -Path compat        # HDR compatibility (screen area, Desktop Duplication)
#   pwsh scripts/judder-ab.ps1 -Path window        # plain window capture
#   pwsh scripts/judder-ab.ps1 -Path hdr-window    # "Capture the window in HDR"
#   pwsh scripts/judder-ab.ps1 -Restore            # put the settings back, stop the test build
#
# Build first (from app\):  node_modules\.bin\vite build
#                           node_modules\.bin\tauri build --no-bundle --config '{\"build\":{\"beforeBuildCommand\":\"\"}}'
#                           cargo build --release -p clip-core --example judder
#
# With PresentMon (Intel's frame-timing tool) at
# target\tools\presentmon\PresentMon-*.exe and an administrator terminal,
# each run also records when the game presented its frames. That tells a
# picture the capture repeated from one the game never drew: "slots
# without a new game frame" is the share of the clip's frame slots in
# which the game presented nothing, which no capture can do better than.
#
# Results: target\judder-ab\ (one .txt and .log per run, summary.csv).
# Works in Windows PowerShell 5.1 and PowerShell 7.

[CmdletBinding()]
param(
    [ValidateSet('compat', 'window', 'hdr-window')]
    [string]$Path,
    # GStreamer to run on: the one the installed Reruns ships (what users
    # get), or the development install.
    [ValidateSet('bundled', 'dev')]
    [string]$Runtime = 'bundled',
    # Length of the clip to save and score.
    [ValidateRange(10, 600)]
    [int]$Seconds = 60,
    # Free text added to the result's name, e.g. the fix being tried.
    [string]$Label = '',
    [switch]$Restore,
    # Summarize an existing PresentMon CSV (all of it) and exit.
    [string]$PresentsCsv = ''
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $repo 'target\release\reruns.exe'
$judder = Join-Path $repo 'target\release\examples\judder.exe'
$results = Join-Path $repo 'target\judder-ab'
$settingsPath = Join-Path $env:APPDATA 'reruns\settings.json'
$backup = Join-Path $results 'settings.before-ab.json'
$log = Join-Path $env:LOCALAPPDATA 'gg.reruns.app\logs\reruns.log'
$installed = Join-Path $env:LOCALAPPDATA 'Reruns'
$devGst = Join-Path $env:LOCALAPPDATA 'Programs\gstreamer\1.0\msvc_x86_64'
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Stop-Reruns {
    $running = @(Get-Process -Name reruns -ErrorAction SilentlyContinue)
    if ($running.Count -eq 0) { return }
    Write-Host "Stopping Reruns ($($running.Count) running)..."
    $running | Stop-Process -Force
    $running | ForEach-Object { try { $_.WaitForExit(5000) | Out-Null } catch {} }
}

# GStreamer's libraries on PATH and its plugins named, for this process
# and the ones it starts.
function Use-Runtime([string]$which) {
    if ($which -eq 'bundled') {
        $plugins = Join-Path $installed 'gst-plugins'
        if (-not (Test-Path $plugins)) { throw "No installed Reruns at $installed; use -Runtime dev." }
        $env:PATH = "$installed;$env:PATH"
        $env:GST_PLUGIN_PATH = $plugins
        $env:GST_PLUGIN_SYSTEM_PATH = $plugins
    } else {
        if (-not (Test-Path "$devGst\bin")) { throw "No GStreamer at $devGst." }
        $env:PATH = "$devGst\bin;$env:PATH"
        Remove-Item Env:GST_PLUGIN_PATH, Env:GST_PLUGIN_SYSTEM_PATH -ErrorAction SilentlyContinue
    }
    # A registry of its own: the two runtimes must not share a plugin cache.
    $env:GST_REGISTRY = Join-Path $results "gst-registry-$which.bin"
}

# The newest PresentMon console build under target\tools, if any.
function Find-PresentMon {
    Get-ChildItem (Join-Path $repo 'target\tools\presentmon') -Filter 'PresentMon-*.exe' -ErrorAction SilentlyContinue |
        Sort-Object Name | Select-Object -Last 1
}

function Test-Admin {
    $me = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    $me.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# The game's own frame pacing from a PresentMon CSV, between `$from` and
# `$to` seconds into the capture: the process that presented most is
# taken for the game. `$fps` is the clip's frame rate.
function Measure-Presents([string]$csvPath, [double]$from, [double]$to, [int]$fps) {
    $notGames = @('dwm.exe', 'reruns.exe', 'msedgewebview2.exe', 'explorer.exe')
    $times = @{}
    foreach ($row in (Import-Csv $csvPath)) {
        $t = $row.TimeInSeconds
        if ($null -eq $t) { $t = $row.CPUStartTime }
        if ($null -eq $t) { continue }
        $t = [double]::Parse($t, [Globalization.CultureInfo]::InvariantCulture)
        if ($t -lt $from -or $t -gt $to) { continue }
        $app = $row.Application
        if ($notGames -contains $app.ToLowerInvariant()) { continue }
        if (-not $times.ContainsKey($app)) { $times[$app] = New-Object 'System.Collections.Generic.List[double]' }
        $times[$app].Add($t)
    }
    $game = $times.GetEnumerator() | Sort-Object { $_.Value.Count } | Select-Object -Last 1
    if (-not $game -or $game.Value.Count -lt 2) { return $null }
    $t = $game.Value.ToArray()
    [Array]::Sort($t)
    $span = $t[$t.Length - 1] - $t[0]
    if ($span -le 0) { return $null }
    # Frame slots of the clip in which the game presented nothing.
    $slots = New-Object 'System.Collections.Generic.HashSet[long]'
    $steps = New-Object 'System.Collections.Generic.List[double]'
    for ($i = 0; $i -lt $t.Length; $i++) {
        [void]$slots.Add([long][Math]::Floor(($t[$i] - $t[0]) * $fps))
        if ($i -gt 0) { $steps.Add(($t[$i] - $t[$i - 1]) * 1000) }
    }
    $total = [long][Math]::Floor($span * $fps) + 1
    $sorted = $steps.ToArray()
    [Array]::Sort($sorted)
    [pscustomobject]@{
        Game       = $game.Key
        Fps        = [Math]::Round(($t.Length - 1) / $span, 1)
        # The frame time 99% of frames are under, as a rate.
        Low1Fps    = [Math]::Round(1000 / $sorted[[int][Math]::Floor(($sorted.Length - 1) * 0.99)], 1)
        LongestMs  = [Math]::Round($sorted[$sorted.Length - 1], 1)
        EmptySlots = [Math]::Round(($total - $slots.Count) * 100.0 / $total, 2)
    }
}

function Format-Presents($p, [int]$fps) {
    if (-not $p) { return 'game frames: none recorded' }
    "game frames ($($p.Game)): $($p.Fps) fps, 1% low $($p.Low1Fps) fps, longest frame $($p.LongestMs) ms; " +
        "$($p.EmptySlots)% of $fps fps slots without a new game frame"
}

if ($PresentsCsv) {
    Write-Host (Format-Presents (Measure-Presents $PresentsCsv 0 ([double]::MaxValue) 60) 60)
    return
}

New-Item -ItemType Directory -Force $results | Out-Null

if ($Restore) {
    Stop-Reruns
    if (Test-Path $backup) {
        Copy-Item $backup $settingsPath -Force
        Remove-Item $backup
        Write-Host "Settings restored. Start Reruns from the Start menu again."
    } else {
        Write-Host "No settings backup found; nothing to restore."
    }
    return
}

if (-not $Path) { throw "Say which path to record: -Path compat | window | hdr-window (or -Restore)." }
foreach ($f in @($exe, $judder, $settingsPath)) {
    if (-not (Test-Path $f)) { throw "Missing $f (see the build steps at the top of this script)." }
}

Stop-Reruns

# The first run keeps the settings as they were, for -Restore.
if (-not (Test-Path $backup)) { Copy-Item $settingsPath $backup }
$settings = [IO.File]::ReadAllText($settingsPath) | ConvertFrom-Json
$settings.hdr_compatibility = ($Path -ne 'window')
$settings.hdr_window_capture = ($Path -eq 'hdr-window')
$settings.replay_seconds = $Seconds
# No byte-order mark: Reruns reads the file as plain JSON.
[IO.File]::WriteAllText($settingsPath, ($settings | ConvertTo-Json -Depth 20), $utf8)
$clips = $settings.clips_dir

Use-Runtime $Runtime
# A local build leaves an empty gst-plugins folder beside the executable
# (the bundle's resource folder, with nothing collected into it). Reruns
# would load its plugins from there, find none, and have no encoder.
$stale = Join-Path (Split-Path -Parent $exe) 'gst-plugins'
if ((Test-Path $stale) -and -not (Get-ChildItem $stale -Filter *.dll)) {
    Remove-Item $stale -Recurse -Force
}
$logStart = 0
if (Test-Path $log) { $logStart = (Get-Item $log).Length }
$started = Get-Date
Start-Process -FilePath $exe -ArgumentList '--autostart' | Out-Null

# The game's own frame times, alongside. Tracing needs administrator
# rights; without them the run goes ahead with the clip's score alone.
$presentMon = Find-PresentMon
$presentCsv = $null
$session = 'RerunsJudderAB'
if ($presentMon -and (Test-Admin)) {
    $presentCsv = Join-Path $results ('{0:yyyy-MM-dd_HH-mm-ss}_{1}_presents.csv' -f $started, $Path)
    $presentArgs = @('--output_file', "`"$presentCsv`"", '--v1_metrics', '--no_console_stats',
        '--session_name', $session, '--stop_existing_session')
    Start-Process -FilePath $presentMon.FullName -ArgumentList $presentArgs -WindowStyle Hidden | Out-Null
    $presentStarted = Get-Date
} elseif ($presentMon) {
    Write-Host "PresentMon needs an administrator terminal; recording without the game's frame times."
} else {
    Write-Host "No PresentMon under target\tools\presentmon; recording without the game's frame times."
}

Write-Host ""
Write-Host "Recording through '$Path' on the $Runtime GStreamer."
Write-Host "Go back to the game, play the same scene for at least $Seconds seconds,"
Write-Host "then press the clip hotkey ($($settings.hotkey)). Waiting for the clip..."

$clip = $null
$deadline = $started.AddMinutes(30)
while (-not $clip) {
    if ((Get-Date) -gt $deadline) { throw "No clip was saved within 30 minutes." }
    Start-Sleep -Seconds 1
    $clip = Get-ChildItem -Path $clips -Recurse -Filter *.mp4 -ErrorAction SilentlyContinue |
        Where-Object { $_.LastWriteTime -gt $started } |
        Sort-Object LastWriteTime | Select-Object -Last 1
}
# The file is renamed into place when it is complete; give the log a moment.
Start-Sleep -Seconds 2

$presents = $null
$presentLine = ''
if ($presentCsv) {
    & $presentMon.FullName --session_name $session --terminate_existing_session 2>&1 | Out-Null
    Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($presentMon.Name)) -ErrorAction SilentlyContinue |
        ForEach-Object { try { $_.WaitForExit(10000) | Out-Null } catch {} }
    if (Test-Path $presentCsv) {
        # The clip is the last `$Seconds` before it was written.
        $end = ($clip.LastWriteTime - $presentStarted).TotalSeconds
        $presents = Measure-Presents $presentCsv ($end - $Seconds - 1) $end ([int]$settings.fps)
    }
    $presentLine = Format-Presents $presents ([int]$settings.fps)
}

$name = '{0:yyyy-MM-dd_HH-mm-ss}_{1}_{2}' -f $started, $Path, $Runtime
if ($Label) { $name = "$name`_$($Label -replace '[^\w.-]', '-')" }
$report = Join-Path $results "$name.txt"
$runLog = Join-Path $results "$name.log"

# What Reruns logged during this run: which path really ran (a failed HDR
# window capture falls back to the plain window) and its own counters.
$logText = ''
if (Test-Path $log) {
    $stream = [IO.File]::Open($log, 'Open', 'Read', 'ReadWrite')
    try {
        if ($stream.Length -lt $logStart) { $logStart = 0 }
        $stream.Position = $logStart
        $logText = (New-Object IO.StreamReader($stream, $utf8)).ReadToEnd()
    } finally { $stream.Dispose() }
}
[IO.File]::WriteAllText($runLog, $logText, $utf8)

$out = & $judder $clip.FullName 2>&1 | ForEach-Object { "$_" }
$logLines = $logText -split "`r?`n" |
    Where-Object { $_ -match 'recorder started|clip pacing|frame pacing|video pacing|falling back|WARN|ERROR' }
$text = @("path: $Path", "runtime: $Runtime", "label: $Label", "clip: $($clip.FullName)", '') +
    $out + @('', $presentLine, '', '--- Reruns log ---') + $logLines
[IO.File]::WriteAllLines($report, [string[]]$text, $utf8)

$score = ''
$summary = $out | Where-Object { $_ -match '^\s+([\d.]+)% judder' } | Select-Object -First 1
if ($summary -match '([\d.]+)% judder') { $score = $Matches[1] }
$csv = Join-Path $results 'summary.csv'
if (-not (Test-Path $csv)) {
    [IO.File]::WriteAllText($csv, "time,path,runtime,label,judder_percent,game_fps,slots_without_game_frame_percent,clip`r`n", $utf8)
}
$gameFps = ''
$emptySlots = ''
if ($presents) { $gameFps = $presents.Fps; $emptySlots = $presents.EmptySlots }
$row = '{0:yyyy-MM-dd HH:mm:ss},{1},{2},"{3}",{4},{5},{6},"{7}"' -f $started, $Path, $Runtime, $Label, $score, $gameFps, $emptySlots, $clip.FullName
[IO.File]::AppendAllText($csv, "$row`r`n", $utf8)

Write-Host ""
$out | Select-Object -First 3 | ForEach-Object { Write-Host $_ }
if ($presentLine) { Write-Host "  $presentLine" }
$logLines | Where-Object { $_ -match 'recorder started' } | Select-Object -Last 1 | ForEach-Object { Write-Host $_ }
Write-Host ""
Write-Host "Saved to $report"
Write-Host "Reruns is still recording through '$Path'. Run the next path, or -Restore when done."
