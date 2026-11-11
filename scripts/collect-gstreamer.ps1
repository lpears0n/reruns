# Copies the GStreamer runtime the Windows build needs into app/src-tauri/gst,
# where tauri.windows.conf.json bundles it next to the executable.
#
#   pwsh scripts/collect-gstreamer.ps1 [-Root $env:GSTREAMER_1_0_ROOT_MSVC_X86_64]
#
# Plugins are limited to the ones Reruns uses, and of the libraries in
# bin/ only those something bundled loads are taken: the plugins' imports,
# their imports' imports and so on, plus the libraries the app itself
# links. (All of bin/ used to be copied: every codec and helper library
# GStreamer ships, most of them for plugins that aren't bundled.)
#
#   -All copies all of bin/ as before, if a pruned bundle ever misbehaves.
param(
  [string]$Root = $env:GSTREAMER_1_0_ROOT_MSVC_X86_64,
  [switch]$All
)
$ErrorActionPreference = 'Stop'
if (-not $Root) { throw 'Set GSTREAMER_1_0_ROOT_MSVC_X86_64 or pass -Root' }

$dest = Join-Path $PSScriptRoot '..\app\src-tauri\gst'
New-Item -ItemType Directory -Force "$dest\bin" | Out-Null
New-Item -ItemType Directory -Force "$dest\plugins" | Out-Null

# The DLL names a PE file imports, delay-loaded ones included, read from
# its import tables (no build tools needed on the machine).
function Get-Imports([string]$file) {
  $b = [IO.File]::ReadAllBytes($file)
  $pe = [BitConverter]::ToInt32($b, 0x3C)
  $sections = [BitConverter]::ToUInt16($b, $pe + 6)
  $optional = $pe + 24
  $plus = [BitConverter]::ToUInt16($b, $optional) -eq 0x20B
  $dirs = $optional + $(if ($plus) { 112 } else { 96 })
  $table = $optional + [BitConverter]::ToUInt16($b, $pe + 20)
  $offset = {
    param([uint32]$rva)
    for ($i = 0; $i -lt $sections; $i++) {
      $s = $table + 40 * $i
      $va = [BitConverter]::ToUInt32($b, $s + 12)
      $size = [Math]::Max([BitConverter]::ToUInt32($b, $s + 8), [BitConverter]::ToUInt32($b, $s + 16))
      if ($rva -ge $va -and $rva -lt $va + $size) { return [int]($rva - $va + [BitConverter]::ToUInt32($b, $s + 20)) }
    }
    return -1
  }
  $name = {
    param([uint32]$rva)
    $at = & $offset $rva
    if ($at -lt 0) { return $null }
    $end = [Array]::IndexOf($b, [byte]0, $at)
    [Text.Encoding]::ASCII.GetString($b, $at, $end - $at)
  }
  $names = @()
  # Directory 1: imports (20-byte entries, name at +12). Directory 13:
  # delay-loaded imports (32-byte entries, name at +4).
  foreach ($d in @(@(1, 20, 12), @(13, 32, 4))) {
    $rva = [BitConverter]::ToUInt32($b, $dirs + 8 * $d[0])
    if ($rva -eq 0) { continue }
    $at = & $offset $rva
    while ($at -ge 0) {
      $n = [BitConverter]::ToUInt32($b, $at + $d[2])
      if ($n -eq 0) { break }
      $names += & $name $n
      $at += $d[1]
    }
  }
  $names | Where-Object { $_ }
}

$plugins = @(
  # core and glue
  'coreelements', 'app', 'typefindfunctions', 'playback', 'videoconvertscale',
  'videorate', 'videocrop', 'videotestsrc', 'videoparsersbad', 'audioconvert',
  'audioresample', 'audiomixer', 'volume', 'audiotestsrc', 'audioparsers',
  'audiorate', 'isomp4', 'libav', 'webp', 'jpeg', 'png',
  # live levels for Settings > Audio
  'level',
  # save sounds: playbin, WAV, MP3/OGG/Opus/FLAC, and an audio output
  'wavparse', 'autodetect', 'ogg', 'vorbis', 'opus', 'flac', 'mpg123', 'directsound',
  # capture and encode on Windows
  'd3d11', 'd3d12', 'nvcodec', 'amfcodec', 'qsv', 'mediafoundation', 'wasapi2',
  'x264', 'openh264', 'fdkaac', 'voaacenc', 'dxgiscreencap'
)
foreach ($p in $plugins) {
  $f = Join-Path $Root "lib\gstreamer-1.0\gst$p.dll"
  if (Test-Path $f) { Copy-Item $f "$dest\plugins\" -Force } else { Write-Host "skipping absent plugin $p" }
}

# Libraries: what the bundled plugins and the app load, followed through.
# The app links these itself (the gstreamer, gstreamer-app and
# gstreamer-video crates and GLib); HDR window capture loads
# GStreamer's D3D11 library by name.
$roots = @(
  'gstreamer-1.0-0.dll', 'gstbase-1.0-0.dll', 'gstapp-1.0-0.dll', 'gstvideo-1.0-0.dll',
  'glib-2.0-0.dll', 'gobject-2.0-0.dll', 'gio-2.0-0.dll', 'gmodule-2.0-0.dll', 'gstd3d11-1.0-0.dll'
)
Remove-Item "$dest\bin\*.dll" -ErrorAction SilentlyContinue
if ($All) {
  Copy-Item "$Root\bin\*.dll" "$dest\bin\" -Force
} else {
  $available = @{}
  Get-ChildItem "$Root\bin" -Filter *.dll | ForEach-Object { $available[$_.Name.ToLowerInvariant()] = $_.FullName }
  $wanted = @{}
  $queue = New-Object System.Collections.Generic.Queue[string]
  Get-ChildItem "$dest\plugins" -Filter *.dll | ForEach-Object { $queue.Enqueue($_.FullName) }
  foreach ($r in $roots) {
    $path = $available[$r.ToLowerInvariant()]
    if ($path) { $wanted[$r.ToLowerInvariant()] = $path; $queue.Enqueue($path) } else { Write-Host "absent library $r" }
  }
  while ($queue.Count -gt 0) {
    foreach ($import in (Get-Imports $queue.Dequeue())) {
      $key = $import.ToLowerInvariant()
      # Windows' own DLLs aren't in bin/ and aren't ours to ship.
      if ($available.ContainsKey($key) -and -not $wanted.ContainsKey($key)) {
        $wanted[$key] = $available[$key]
        $queue.Enqueue($available[$key])
      }
    }
  }
  $wanted.Values | ForEach-Object { Copy-Item $_ "$dest\bin\" -Force }
  $whole = (Get-ChildItem "$Root\bin" -Filter *.dll | Measure-Object Length -Sum)
  $kept = (Get-ChildItem "$dest\bin" -Filter *.dll | Measure-Object Length -Sum)
  Write-Host ("libraries: kept {0} of {1} ({2:N0} of {3:N0} MB)" -f $kept.Count, $whole.Count, ($kept.Sum / 1MB), ($whole.Sum / 1MB))
}
Write-Host "collected $(@(Get-ChildItem "$dest\bin" -Filter *.dll).Count) libraries and $(@(Get-ChildItem "$dest\plugins" -Filter *.dll).Count) plugins"

# Every element the app makes by name must come from the bundled plugins
# alone (hardware encoders and decoders are left out: they only appear on a
# machine with that GPU). A missing one here would otherwise only show up as
# "no working encoder" on a user's machine.
$required = @(
  'videotestsrc', 'audiotestsrc', 'capsfilter', 'fakesink', 'filesink', 'queue', 'appsrc', 'appsink',
  'videoconvert', 'videoscale', 'videorate', 'videocrop',
  'h264parse', 'h265parse', 'av1parse', 'aacparse', 'mp4mux', 'uridecodebin', 'decodebin',
  'audioconvert', 'audioresample', 'audiomixer', 'audiorate', 'volume', 'level',
  'playbin', 'wavparse', 'autoaudiosink', 'wasapi2sink',
  'd3d11screencapturesrc', 'd3d11convert', 'd3d11upload', 'wasapi2src',
  'pngenc', 'jpegenc', 'x264enc', 'mfh264enc', 'avdec_h264', 'avenc_aac'
)
$env:GST_PLUGIN_PATH = "$dest\plugins"
$env:GST_PLUGIN_SYSTEM_PATH = "$dest\plugins"
$env:GST_REGISTRY = Join-Path $env:TEMP 'reruns-bundle-registry.bin'
Remove-Item $env:GST_REGISTRY -ErrorAction SilentlyContinue
# Checked the way a user's PC runs it: gst-inspect from a copy beside
# the bundled libraries, with GStreamer's own bin/ off the PATH, so a
# library the pruning wrongly left out fails here and not there.
$check = Join-Path $env:TEMP 'reruns-bundle-check'
Remove-Item $check -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $check | Out-Null
Copy-Item "$dest\bin\*.dll" $check
Copy-Item "$Root\bin\gst-inspect-1.0.exe" $check
$inspect = Join-Path $check 'gst-inspect-1.0.exe'
$env:PATH = (($env:PATH -split ';') | Where-Object { $_ -and ($_.TrimEnd('\') -ne "$Root\bin".TrimEnd('\')) }) -join ';'
$missing = @()
foreach ($e in $required) {
  & $inspect $e *> $null
  if ($LASTEXITCODE -ne 0) { $missing += $e }
}
if ($missing.Count -gt 0) { throw "elements missing from the bundle: $($missing -join ', ')" }
Write-Host "all $($required.Count) required elements load from the bundle"

# Optional features: the app falls back without them, so warn rather than
# fail. HDR compatibility (tone-mapped desktop duplication) needs these.
# HDR window capture loads GStreamer's D3D11 library itself.
if (-not (Test-Path "$dest\bin\gstd3d11-1.0-0.dll")) { Write-Warning 'gstd3d11-1.0-0.dll missing from the bundle (HDR window capture falls back)' }
foreach ($e in @('d3d12screencapturesrc', 'd3d12convert', 'd3d12download')) {
  & $inspect $e *> $null
  if ($LASTEXITCODE -ne 0) { Write-Warning "optional element missing from the bundle: $e (HDR compatibility falls back to window capture)" }
}
